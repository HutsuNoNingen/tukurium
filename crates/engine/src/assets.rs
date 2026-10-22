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
#[derive(Clone)]
enum Waiting {
    Ready(u64),
    Download(u64),
}
pub struct Assets {
    root: PathBuf,
    incoming: HashMap<(Origin, String), Incoming>,
    waiting: HashMap<String, Vec<Waiting>>,
}
impl Assets {
    pub fn new(root: PathBuf) -> std::io::Result<Self> {
        std::fs::create_dir_all(&root)?;
        Ok(Self {
            root,
            incoming: HashMap::new(),
            waiting: HashMap::new(),
        })
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
pub fn validate_glb(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() < 20
        || bytes.len() > MAX_ASSET
        || &bytes[..4] != b"glTF"
        || u32::from_le_bytes(bytes[4..8].try_into().unwrap()) != 2
        || u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize != bytes.len()
    {
        return Err("Choose a self-contained glTF 2.0 binary (.glb), up to 128 MiB".into());
    }
    let mut offset = 12;
    let mut document = None;
    while offset < bytes.len() {
        if offset + 8 > bytes.len() {
            return Err("Truncated GLB chunk".into());
        }
        let length = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
        let kind = &bytes[offset + 4..offset + 8];
        let end = (offset + 8)
            .checked_add(length)
            .filter(|end| *end <= bytes.len())
            .ok_or("Invalid GLB chunk length")?;
        if offset == 12 && kind != b"JSON" {
            return Err("GLB must start with JSON".into());
        }
        if kind == b"JSON" {
            if document.is_some() {
                return Err("Duplicate GLB document".into());
            }
            document = Some(
                serde_json::from_slice::<Value>(&bytes[offset + 8..end])
                    .map_err(|_| "Invalid GLB JSON")?,
            );
        }
        offset = end;
    }
    let document = document.ok_or("Missing GLB document")?;
    if document["asset"]["version"] != "2.0"
        || !document["meshes"].as_array().is_some_and(|m| !m.is_empty())
    {
        return Err("GLB must contain a glTF 2.0 mesh".into());
    }
    for collection in ["buffers", "images"] {
        if let Some(items) = document[collection].as_array() {
            for item in items {
                if let Some(uri) = item.get("uri")
                    && (collection == "buffers"
                        || !uri.as_str().is_some_and(|s| {
                            s.starts_with("data:image/png;base64,")
                                || s.starts_with("data:image/jpeg;base64,")
                        }))
                {
                    return Err(
                        "GLB dependencies must be embedded; external files are unsupported".into(),
                    );
                }
            }
        }
    }
    Ok(())
}

impl Engine {
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
                self.assets
                    .waiting
                    .entry(id.into())
                    .or_default()
                    .push(Waiting::Ready(client));
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
                    Waiting::Download(client) => self.offer_asset(Origin::Local(client), id),
                }
            }
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
                    if offset == incoming.size {
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
                    } else if let Origin::Local(client) = from {
                        if self.host {
                            return Err("This model is missing from the host asset cache".into());
                        }
                        let waiting = self.assets.waiting.entry(id.into()).or_default();
                        let first = waiting.is_empty();
                        if !waiting
                            .iter()
                            .any(|w| matches!(w,Waiting::Download(c) if c==&client))
                        {
                            waiting.push(Waiting::Download(client));
                        }
                        if first {
                            self.network(
                                Some(&self.host_peer.clone()),
                                json!({"kind":"asset_get","asset_id":id}),
                            );
                        }
                    } else {
                        return Err("Asset is not available".into());
                    }
                }
                "asset_ready" => {
                    if matches!(&from,Origin::Peer(peer) if peer==&self.host_peer) {
                        self.finish_asset_waiters(id, true);
                    }
                }
                "asset_error" => {
                    if let Some(waiting) = self.assets.waiting.remove(id) {
                        for waiter in waiting {
                            let client = match waiter {
                                Waiting::Ready(c) | Waiting::Download(c) => c,
                            };
                            self.asset_reply(Origin::Local(client), body.clone());
                        }
                    }
                    self.assets.incoming.remove(&key);
                }
                _ => {}
            }
            Ok(())
        })();
        if let Err(message) = result {
            self.assets.incoming.remove(&key);
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
        }
    }
    pub(super) fn close_asset_client(&mut self, client: u64) {
        self.assets
            .incoming
            .retain(|(source, _), _| *source != Origin::Local(client));
        self.assets.waiting.retain(|_, waiting| {
            waiting.retain(|w| !matches!(w,Waiting::Ready(c)|Waiting::Download(c) if *c==client));
            !waiting.is_empty()
        });
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
