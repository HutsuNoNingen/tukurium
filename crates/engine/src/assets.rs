//! Content-addressed GLB storage and paced, restartable transfer outside world snapshots.
use super::{Engine, Origin};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    io::{Read, Seek, SeekFrom},
    path::PathBuf,
    time::{Duration, Instant},
};

pub const MAX_ASSET: usize = 128 * 1024 * 1024;
const CHUNK: usize = 128 * 1024;
struct Incoming {
    bytes: Vec<u8>,
    size: usize,
    touched: Instant,
}
#[derive(Clone, PartialEq)]
enum Waiting {
    Ready(u64),
    Download(Origin),
}
pub struct Assets {
    root: PathBuf,
    incoming: HashMap<(Origin, String), Incoming>,
    waiting: HashMap<String, Vec<Waiting>>,
    waiting_touched: HashMap<String, Instant>,
}
impl Assets {
    pub fn new(root: PathBuf) -> std::io::Result<Self> {
        std::fs::create_dir_all(&root)?;
        Ok(Self {
            root,
            incoming: HashMap::new(),
            waiting: HashMap::new(),
            waiting_touched: HashMap::new(),
        })
    }
    fn wait_for(&mut self, id: &str, waiter: Waiting) -> Result<bool, String> {
        if !self.waiting.contains_key(id) && self.waiting.len() >= 64 {
            return Err("Too many pending assets".into());
        }
        let waiting = self.waiting.entry(id.into()).or_default();
        let first = waiting.is_empty();
        if !waiting.contains(&waiter) {
            if waiting.len() >= 32 {
                return Err("Too many asset subscribers".into());
            }
            waiting.push(waiter);
        }
        self.waiting_touched.insert(id.into(), Instant::now());
        Ok(first)
    }
    fn path(&self, id: &str) -> PathBuf {
        let texture = self.root.join(format!("{id}.png"));
        if texture.is_file() {
            return texture;
        }
        let video = self.root.join(format!("{id}.ogv"));
        if video.is_file() {
            video
        } else {
            self.root.join(format!("{id}.glb"))
        }
    }
    pub fn contains(&self, id: &str) -> bool {
        valid_id(id) && self.path(id).is_file()
    }
    pub fn is_avatar_texture(&self, id: &str) -> bool {
        self.contains(id) && self.path(id).extension().is_some_and(|e| e == "png")
    }
    fn size(&self, id: &str) -> Result<usize, String> {
        if !valid_id(id) {
            return Err("Invalid asset ID".into());
        }
        let size = std::fs::metadata(self.path(id))
            .map_err(|_| "Asset is not available on the host")?
            .len() as usize;
        if !(20..=MAX_ASSET).contains(&size) {
            return Err("Invalid asset size".into());
        }
        Ok(size)
    }
    fn save(&self, id: &str, bytes: &[u8]) -> Result<(), String> {
        if format!("{:x}", Sha256::digest(bytes)) != id {
            return Err("Asset checksum mismatch".into());
        }
        let format = validate_asset(bytes)?;
        let temporary = self.root.join(format!("{}.part", uuid::Uuid::new_v4()));
        std::fs::write(&temporary, bytes).map_err(|e| e.to_string())?;
        if self.path(id).exists() {
            let _ = std::fs::remove_file(&temporary);
            return Ok(());
        }
        std::fs::rename(&temporary, self.root.join(format!("{id}.{format}")))
            .map_err(|e| e.to_string())
    }
}
fn valid_id(id: &str) -> bool {
    id.len() == 64
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn validate_asset(bytes: &[u8]) -> Result<&'static str, String> {
    if bytes.len() > MAX_ASSET {
        return Err("Converted asset exceeds 128 MiB".into());
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        if bytes.len() > 1024 * 1024 {
            return Err("Avatar texture exceeds 1 MiB".into());
        }
        let reader =
            image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Png);
        let (width, height) = reader.into_dimensions().map_err(|_| "Invalid avatar PNG")?;
        if width == 0 || height == 0 || width > 2048 || height > 2048 {
            return Err("Avatar texture dimensions exceed 2048".into());
        }
        image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
            .map_err(|_| "Invalid avatar PNG")?;
        return Ok("png");
    }
    if bytes.starts_with(b"OggS")
        && bytes[..bytes.len().min(65536)]
            .windows(7)
            .any(|w| w == b"\x80theora")
    {
        return Ok("ogv");
    }
    validate_glb(bytes)?;
    Ok("glb")
}
pub use kokoverse_core::asset_validation::validate_glb;

impl Engine {
    fn asset_progress(&mut self, id: &str, phase: &str, completed: usize, total: usize) {
        let body = json!({"kind":"asset_transfer","asset_id":id,"phase":phase,"completed":completed,"total":total});
        self.ui(body.clone());
        if let Some(touched) = self.assets.waiting_touched.get_mut(id) {
            *touched = Instant::now();
        }
        let subscribers: Vec<_> = self
            .assets
            .waiting
            .get(id)
            .into_iter()
            .flatten()
            .filter_map(|w| match w {
                Waiting::Download(Origin::Peer(peer)) => Some(peer.clone()),
                _ => None,
            })
            .collect();
        for peer in subscribers {
            self.asset_reply(Origin::Peer(peer), body.clone());
        }
    }
    fn fail_asset_waiters(&mut self, id: &str, message: &str) {
        self.assets.waiting_touched.remove(id);
        if let Some(waiting) = self.assets.waiting.remove(id) {
            for waiter in waiting {
                let target = match waiter {
                    Waiting::Ready(client) => Origin::Local(client),
                    Waiting::Download(origin) => origin,
                };
                self.asset_reply(
                    target,
                    json!({"kind":"asset_error","asset_id":id,"message":message}),
                );
            }
        }
    }
    fn asset_reply(&mut self, to: Origin, body: Value) {
        match to {
            Origin::Local(id) => {
                let value = self.envelope(body);
                self.send(id, value);
            }
            Origin::Peer(peer) => self.network(Some(&peer), body),
        }
    }
    fn offer_asset(&mut self, to: Origin, id: &str) {
        match self.assets.size(id) {
            Ok(size) => {
                let path = self.assets.path(id);
                let format = path.extension().and_then(|s| s.to_str()).unwrap_or("glb");
                self.asset_reply(
                    to,
                    json!({"kind":"asset_offer","asset_id":id,"size":size,"format":format}),
                )
            }
            Err(message) => self.asset_reply(
                to,
                json!({"kind":"asset_error","asset_id":id,"message":message}),
            ),
        }
    }
    fn asset_received(&mut self, from: Origin, id: &str) {
        match from {
            Origin::Local(client) if !self.host => {
                if let Err(message) = self.assets.wait_for(id, Waiting::Ready(client)) {
                    self.asset_reply(
                        Origin::Local(client),
                        json!({"kind":"asset_error","asset_id":id,"message":message}),
                    );
                    return;
                }
                self.asset_progress(id, "uploading_host", 0, self.assets.size(id).unwrap_or(0));
                self.offer_asset(Origin::Peer(self.host_peer.clone()), id);
            }
            source => self.asset_reply(source, json!({"kind":"asset_ready","asset_id":id})),
        }
        self.finish_asset_waiters(id, false);
    }
    fn finish_asset_waiters(&mut self, id: &str, host_ready: bool) {
        if let Some(waiting) = self.assets.waiting.remove(id) {
            for waiter in waiting {
                match waiter {
                    Waiting::Ready(client) if host_ready => self.asset_reply(
                        Origin::Local(client),
                        json!({"kind":"asset_ready","asset_id":id}),
                    ),
                    Waiting::Ready(client) => self
                        .assets
                        .waiting
                        .entry(id.into())
                        .or_default()
                        .push(Waiting::Ready(client)),
                    Waiting::Download(origin) => self.offer_asset(origin, id),
                }
            }
        }
        if !self.assets.waiting.contains_key(id) {
            self.assets.waiting_touched.remove(id);
        }
    }
    pub(super) fn asset_message(&mut self, from: Origin, body: &Value) {
        let Some(id) = body["asset_id"].as_str().filter(|s| valid_id(s)) else {
            return;
        };
        let key = (from.clone(), id.to_string());
        let result = (|| -> Result<(), String> {
            match body["kind"].as_str().unwrap_or("") {
                "asset_offer" => {
                    let size = body["size"]
                        .as_u64()
                        .filter(|s| (20..=MAX_ASSET as u64).contains(s))
                        .ok_or("Converted asset exceeds 128 MiB")?
                        as usize;
                    if self.assets.contains(id) {
                        self.asset_received(from.clone(), id);
                        return Ok(());
                    }
                    if let Some(incoming) = self.assets.incoming.get(&key) {
                        self.asset_reply(from.clone(),json!({"kind":"asset_chunk_request","asset_id":id,"offset":incoming.bytes.len()}));
                        return Ok(());
                    }
                    if self.assets.incoming.len() >= 4 && !self.assets.incoming.contains_key(&key) {
                        return Err("Too many simultaneous imports".into());
                    }
                    self.assets.incoming.insert(
                        key.clone(),
                        Incoming {
                            bytes: Vec::new(),
                            size,
                            touched: Instant::now(),
                        },
                    );
                    if self.host || matches!(&from, Origin::Peer(_)) {
                        self.asset_progress(
                            id,
                            if self.host {
                                "waiting_upload"
                            } else {
                                "downloading_network"
                            },
                            0,
                            size,
                        );
                    }
                    self.asset_reply(
                        from.clone(),
                        json!({"kind":"asset_chunk_request","asset_id":id,"offset":0}),
                    );
                }
                "asset_chunk_request" => {
                    let size = self.assets.size(id)?;
                    let offset = body["offset"]
                        .as_u64()
                        .filter(|n| *n < size as u64)
                        .ok_or("Invalid chunk offset")? as usize;
                    if matches!(&from, Origin::Peer(peer) if peer == &self.host_peer)
                        && self.assets.waiting.get(id).is_some_and(|waiting| {
                            waiting.iter().any(|w| matches!(w, Waiting::Ready(_)))
                        })
                    {
                        self.asset_progress(id, "uploading_host", offset, size);
                    }
                    let mut file =
                        std::fs::File::open(self.assets.path(id)).map_err(|e| e.to_string())?;
                    file.seek(SeekFrom::Start(offset as u64))
                        .map_err(|e| e.to_string())?;
                    let mut data = vec![0; CHUNK.min(size - offset)];
                    file.read_exact(&mut data).map_err(|e| e.to_string())?;
                    self.asset_reply(from.clone(),json!({"kind":"asset_chunk","asset_id":id,"offset":offset,"data":STANDARD.encode(data)}));
                }
                "asset_chunk" => {
                    let text = body["data"]
                        .as_str()
                        .filter(|s| s.len() <= CHUNK * 4 / 3 + 4)
                        .ok_or("Invalid chunk size")?;
                    let data = STANDARD
                        .decode(text)
                        .map_err(|_| "Invalid chunk encoding")?;
                    let incoming = self
                        .assets
                        .incoming
                        .get_mut(&key)
                        .ok_or("No active asset transfer")?;
                    if body["offset"].as_u64() != Some(incoming.bytes.len() as u64)
                        || data.is_empty()
                        || data.len() > CHUNK
                        || incoming.bytes.len() + data.len() > incoming.size
                    {
                        return Err("Out-of-order asset chunk".into());
                    }
                    incoming.bytes.extend(data);
                    incoming.touched = Instant::now();
                    let offset = incoming.bytes.len();
                    let total = incoming.size;
                    if self.host || matches!(&from, Origin::Peer(_)) {
                        self.asset_progress(
                            id,
                            if self.host {
                                "waiting_upload"
                            } else {
                                "downloading_network"
                            },
                            offset,
                            total,
                        );
                    }
                    if offset == total {
                        let incoming = self.assets.incoming.remove(&key).unwrap();
                        self.assets.save(id, &incoming.bytes)?;
                        self.asset_received(from.clone(), id);
                    } else {
                        self.asset_reply(
                            from.clone(),
                            json!({"kind":"asset_chunk_request","asset_id":id,"offset":offset}),
                        );
                    }
                }
                "asset_get" => {
                    if self.assets.contains(id) {
                        self.offer_asset(from.clone(), id);
                    } else if self.host || matches!(&from, Origin::Local(_)) {
                        let first = self.assets.wait_for(id, Waiting::Download(from.clone()))?;
                        let progress = self
                            .assets
                            .incoming
                            .iter()
                            .find(|((_, asset), _)| asset == id)
                            .map(|(_, v)| (v.bytes.len(), v.size))
                            .unwrap_or((0, 0));
                        self.asset_reply(from.clone(),json!({"kind":"asset_transfer","asset_id":id,"phase":"waiting_upload","completed":progress.0,"total":progress.1}));
                        if first && !self.host {
                            self.network(
                                Some(&self.host_peer.clone()),
                                json!({"kind":"asset_get","asset_id":id}),
                            );
                        }
                    } else {
                        return Err("Asset is not available".into());
                    }
                }
                "asset_transfer" => {
                    if matches!(&from,Origin::Peer(peer) if peer==&self.host_peer) {
                        if let Some(touched) = self.assets.waiting_touched.get_mut(id) {
                            *touched = Instant::now();
                        }
                        self.ui(body.clone());
                    }
                }
                "asset_ready" => {
                    if matches!(&from,Origin::Peer(peer) if peer==&self.host_peer) {
                        let size = self.assets.size(id).unwrap_or(0);
                        self.asset_progress(id, "waiting_host", size, size);
                        self.finish_asset_waiters(id, true);
                    }
                }
                "asset_error" => {
                    self.fail_asset_waiters(
                        id,
                        body["message"].as_str().unwrap_or("Asset transfer failed"),
                    );
                    self.assets.incoming.remove(&key);
                }
                _ => {}
            }
            Ok(())
        })();
        if let Err(message) = result {
            self.assets.incoming.remove(&key);
            self.fail_asset_waiters(id, &message);
            self.asset_reply(
                from,
                json!({"kind":"asset_error","asset_id":id,"message":message}),
            );
        }
    }
    pub(super) fn expire_assets(&mut self) {
        let stale: Vec<_> = self
            .assets
            .incoming
            .iter()
            .filter(|(_, v)| v.touched.elapsed() > Duration::from_secs(30))
            .map(|(k, _)| k.clone())
            .collect();
        for (source, id) in stale {
            self.assets.incoming.remove(&(source.clone(), id.clone()));
            self.asset_reply(source,json!({"kind":"asset_error","asset_id":id,"message":"Asset transfer timed out; import again"}));
            self.fail_asset_waiters(&id, "Asset upload timed out");
        }
        let waiting: Vec<_> = self
            .assets
            .waiting_touched
            .iter()
            .filter(|(_, at)| at.elapsed() > Duration::from_secs(30))
            .map(|(id, _)| id.clone())
            .collect();
        for id in waiting {
            self.fail_asset_waiters(
                &id,
                "Asset has not arrived. Retry after the uploader reconnects.",
            );
        }
    }
    pub(super) fn close_asset_client(&mut self, client: u64) {
        self.assets
            .incoming
            .retain(|(source, _), _| *source != Origin::Local(client));
        self.assets.waiting.retain(|_, waiting| {
            waiting.retain(|w| match w {
                Waiting::Ready(c) => *c != client,
                Waiting::Download(origin) => *origin != Origin::Local(client),
            });
            !waiting.is_empty()
        });
        self.assets
            .waiting_touched
            .retain(|id, _| self.assets.waiting.contains_key(id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn glb(document: Value) -> Vec<u8> {
        let mut data = serde_json::to_vec(&document).unwrap();
        while !data.len().is_multiple_of(4) {
            data.push(b' ');
        }
        let mut bytes = b"glTF".to_vec();
        bytes.extend(2u32.to_le_bytes());
        bytes.extend((20u32 + data.len() as u32).to_le_bytes());
        bytes.extend((data.len() as u32).to_le_bytes());
        bytes.extend(b"JSON");
        bytes.extend(data);
        bytes
    }

    #[test]
    fn missing_asset_subscriptions_are_bounded_and_deduplicated() {
        let root = std::env::temp_dir().join(format!("kokoverse-waiters-{}", uuid::Uuid::new_v4()));
        let mut assets = Assets::new(root.clone()).unwrap();
        let waiter = Waiting::Download(Origin::Peer("guest".into()));
        assert!(assets.wait_for("asset", waiter.clone()).unwrap());
        assert!(!assets.wait_for("asset", waiter).unwrap());
        assert_eq!(assets.waiting["asset"].len(), 1);
        for client in 0..31 {
            assets
                .wait_for("asset", Waiting::Download(Origin::Local(client)))
                .unwrap();
        }
        assert!(
            assets
                .wait_for("asset", Waiting::Download(Origin::Local(99)))
                .is_err()
        );
        for id in 0..63 {
            assets.wait_for(&id.to_string(), Waiting::Ready(1)).unwrap();
        }
        assert!(assets.wait_for("overflow", Waiting::Ready(1)).is_err());
        std::fs::remove_dir(root).unwrap();
    }
    #[test]
    fn glb_checks_reject_external_dependencies_and_truncated_chunks() {
        let valid = glb(json!({"asset":{"version":"2.0"},"meshes":[{}]}));
        assert!(validate_glb(&valid).is_ok());
        assert!(validate_glb(&valid[..valid.len() - 1]).is_err());
        let external = glb(
            json!({"asset":{"version":"2.0"},"meshes":[{}],"images":[{"uri":"file:///private.png"}]}),
        );
        assert!(validate_glb(&external).is_err());
        let external = glb(
            json!({"asset":{"version":"2.0"},"meshes":[{}],"buffers":[{"uri":"https://example.com/model.bin"}]}),
        );
        assert!(validate_glb(&external).is_err());
    }
    #[test]
    fn cache_validates_content_hash_and_survives_reopen() {
        let root = std::env::temp_dir().join(format!("kokoverse-assets-{}", uuid::Uuid::new_v4()));
        let store = Assets::new(root.clone()).unwrap();
        let bytes = glb(json!({"asset":{"version":"2.0"},"meshes":[{}]}));
        let id = format!("{:x}", Sha256::digest(&bytes));
        assert!(store.save(&"0".repeat(64), &bytes).is_err());
        assert!(store.size("../outside").is_err());
        store.save(&id, &bytes).unwrap();
        let reopened = Assets::new(root.clone()).unwrap();
        assert!(reopened.contains(&id));
        assert_eq!(std::fs::read(reopened.path(&id)).unwrap(), bytes);
        std::fs::remove_file(reopened.path(&id)).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
    #[test]
    fn theora_assets_use_a_video_cache_file() {
        let root = std::env::temp_dir().join(format!("kokoverse-video-{}", uuid::Uuid::new_v4()));
        let store = Assets::new(root.clone()).unwrap();
        let bytes = b"OggS\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x80theora";
        let id = format!("{:x}", Sha256::digest(bytes));
        store.save(&id, bytes).unwrap();
        assert_eq!(store.path(&id).extension().unwrap(), "ogv");
        assert!(store.contains(&id));
        assert!(validate_asset(b"OggS\0\0invalid-codec").is_err());
        std::fs::remove_file(store.path(&id)).unwrap();
        std::fs::remove_dir(root).unwrap();
    }

    #[test]
    fn avatar_png_is_bounded_validated_and_cached_separately() {
        let mut data = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(32, 32)
            .write_to(&mut data, image::ImageFormat::Png)
            .unwrap();
        let data = data.into_inner();
        assert_eq!(validate_asset(&data).unwrap(), "png");
        let root = std::env::temp_dir().join(format!("kokoverse-avatar-{}", uuid::Uuid::new_v4()));
        let store = Assets::new(root.clone()).unwrap();
        let id = format!("{:x}", Sha256::digest(&data));
        store.save(&id, &data).unwrap();
        assert!(store.is_avatar_texture(&id));
        assert_eq!(store.path(&id).extension().unwrap(), "png");
        assert!(validate_asset(&data[..20]).is_err());
        let mut big = data.clone();
        big.resize(1024 * 1024 + 1, 0);
        assert!(validate_asset(&big).is_err());
        let mut wide = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(2049, 1)
            .write_to(&mut wide, image::ImageFormat::Png)
            .unwrap();
        assert!(validate_asset(&wide.into_inner()).is_err());
        std::fs::remove_file(store.path(&id)).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
