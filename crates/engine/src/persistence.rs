//! Host-only checkpoints. Disk I/O runs off the simulation thread.
use kokoverse_core::{WorldModel, replication::REGISTRY};
use kokoverse_protocol::{MAX_FRAME, ReplicatedWorld, Snapshot};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use tokio::task::JoinHandle;

pub fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".bak");
    name.into()
}

pub fn read_snapshot(path: &Path) -> Result<Snapshot, String> {
    let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(MAX_FRAME as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_FRAME {
        return Err("Saved world exceeds size limit".into());
    }
    let stored: ReplicatedWorld = serde_json::from_slice(&bytes)
        .map_err(|e| format!("Unsupported or invalid world file: {e}"))?;
    let snapshot = REGISTRY.decode_world(stored)?;
    WorldModel::new().restore(snapshot.clone())?;
    Ok(snapshot)
}

pub fn load_checkpoint(path: &Path) -> Result<Snapshot, String> {
    match read_snapshot(path) {
        Ok(snapshot) => Ok(snapshot),
        Err(error) => match read_snapshot(&backup_path(path)) {
            Ok(snapshot) => {
                eprintln!(
                    "World checkpoint unreadable ({error}); recovered {}",
                    backup_path(path).display()
                );
                Ok(snapshot)
            }
            Err(backup_error) => Err(format!(
                "Cannot restore world: {error}; backup: {backup_error}"
            )),
        },
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let temporary = parent.join(format!(".world-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        // rename replaces an existing destination atomically on Windows and Unix.
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(|e| format!("{}: {e}", path.display()))
}

fn write_checkpoint(path: &Path, snapshot: &Snapshot) -> Result<(), String> {
    let bytes = serde_json::to_vec(&REGISTRY.encode_world(snapshot)).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_FRAME {
        return Err("Saved world exceeds size limit".into());
    }
    // Keep the previous primary as the backup, copying its bytes as-is
    // instead of decoding, fully validating and re-encoding it on every
    // save; the full restore validation runs at load time. Never replace a
    // good backup with a primary that does not even parse (e.g. a torn
    // write recovered at startup).
    if let Ok(previous) = fs::read(path)
        && previous.len() <= MAX_FRAME
        && serde_json::from_slice::<ReplicatedWorld>(&previous).is_ok()
    {
        atomic_write(&backup_path(path), &previous)?;
    }
    atomic_write(path, &bytes)
}

#[derive(Default)]
pub struct Persistence {
    path: Option<PathBuf>,
    saved_revision: Option<String>,
    pending: Option<JoinHandle<Result<String, String>>>,
}

impl Persistence {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self {
            path,
            ..Self::default()
        }
    }

    pub async fn poll(&mut self) -> Result<(), String> {
        if self.pending.as_ref().is_some_and(|p| p.is_finished()) {
            self.finish().await?;
        }
        Ok(())
    }

    async fn finish(&mut self) -> Result<(), String> {
        if let Some(pending) = self.pending.take() {
            self.saved_revision = Some(pending.await.map_err(|e| e.to_string())??);
        }
        Ok(())
    }

    pub fn schedule(&mut self, snapshot: Snapshot) {
        let revision = snapshot.world_revision.clone();
        self.schedule_with(&revision, move || snapshot);
    }

    /// Schedule a save for `revision`, building the snapshot only when a
    /// write will actually happen. Callers compare revisions first so an
    /// unchanged world does not pay for a full snapshot clone every tick.
    pub fn schedule_with(&mut self, revision: &str, snapshot: impl FnOnce() -> Snapshot) {
        if self.pending.is_some() || self.saved_revision.as_deref() == Some(revision) {
            return;
        }
        let Some(path) = self.path.clone() else {
            return;
        };
        let snapshot = snapshot();
        self.pending = Some(tokio::task::spawn_blocking(move || {
            write_checkpoint(&path, &snapshot)?;
            Ok(snapshot.world_revision)
        }));
    }

    pub async fn flush(&mut self, snapshot: Snapshot) -> Result<bool, String> {
        if self.path.is_none() {
            return Ok(false);
        }
        // A failed older write must not prevent a new explicit save attempt.
        if let Err(error) = self.finish().await {
            eprintln!("World autosave failed: {error}");
        }
        self.schedule(snapshot);
        self.finish().await?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!("kokoverse-save-{}", uuid::Uuid::new_v4())))
        }
        fn world(&self) -> PathBuf {
            self.0.join("world.json")
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[tokio::test]
    async fn checkpoints_replace_existing_files_and_keep_previous_good_backup() {
        let directory = Directory::new();
        let path = directory.world();
        let mut persistence = Persistence::new(Some(path.clone()));
        let initial = WorldModel::new().snapshot();
        assert!(persistence.flush(initial.clone()).await.unwrap());
        let mut updated = initial.clone();
        updated.world_revision = "1".into();
        persistence.flush(updated.clone()).await.unwrap();
        assert_eq!(read_snapshot(&path).unwrap().world_revision, "1");
        assert_eq!(
            read_snapshot(&backup_path(&path)).unwrap().world_revision,
            "0"
        );
        let before = fs::metadata(&path).unwrap().modified().unwrap();
        persistence.flush(updated).await.unwrap();
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), before);
        fs::write(&path, b"interrupted file").unwrap();
        assert_eq!(load_checkpoint(&path).unwrap().world_revision, "0");
        let mut restarted = Persistence::new(Some(path.clone()));
        restarted.flush(initial).await.unwrap();
        assert_eq!(read_snapshot(&path).unwrap().world_revision, "0");
        assert_eq!(
            read_snapshot(&backup_path(&path)).unwrap().world_revision,
            "0"
        );
    }

    #[tokio::test]
    async fn failure_does_not_discard_last_good_checkpoint_and_can_retry() {
        let directory = Directory::new();
        let path = directory.world();
        let initial = WorldModel::new().snapshot();
        let mut persistence = Persistence::new(Some(path.clone()));
        persistence.flush(initial.clone()).await.unwrap();
        fs::create_dir(backup_path(&path)).unwrap();
        let mut updated = initial.clone();
        updated.world_revision = "1".into();
        assert!(persistence.flush(updated.clone()).await.is_err());
        assert_eq!(read_snapshot(&path).unwrap().world_revision, "0");
        fs::remove_dir(backup_path(&path)).unwrap();
        persistence.flush(updated).await.unwrap();
        assert_eq!(read_snapshot(&path).unwrap().world_revision, "1");
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 2);
    }

    #[tokio::test]
    async fn schedule_with_skips_snapshot_build_when_unchanged() {
        let directory = Directory::new();
        let path = directory.world();
        let mut persistence = Persistence::new(Some(path.clone()));
        let initial = WorldModel::new().snapshot();
        assert!(persistence.flush(initial.clone()).await.unwrap());
        // Same revision as already saved: the provider must not run at all.
        persistence.schedule_with(&initial.world_revision, || {
            panic!("snapshot built for an unchanged world")
        });
        persistence.poll().await.unwrap();
        // A new revision does build and save.
        let mut updated = initial.clone();
        updated.world_revision = "1".into();
        let provided = updated.clone();
        persistence.schedule_with("1", move || provided);
        while persistence
            .pending
            .as_ref()
            .is_some_and(|p| !p.is_finished())
        {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        persistence.poll().await.unwrap();
        assert_eq!(read_snapshot(&path).unwrap().world_revision, "1");
    }

    #[test]
    fn invalid_world_is_not_silently_replaced() {
        let directory = Directory::new();
        fs::create_dir_all(&directory.0).unwrap();
        fs::write(
            directory.world(),
            br#"{"format_version":1,"world_revision":"0","objects":[]}"#,
        )
        .unwrap();
        // Empty worlds are valid; malformed snapshots and overlarge files are not.
        assert!(read_snapshot(&directory.world()).is_ok());
        fs::write(directory.world(), b"{}").unwrap();
        assert!(load_checkpoint(&directory.world()).is_err());
        fs::write(directory.world(), vec![b' '; MAX_FRAME + 1]).unwrap();
        assert!(
            read_snapshot(&directory.world())
                .unwrap_err()
                .contains("size limit")
        );
    }
}
