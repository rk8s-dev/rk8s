//! Per-volume local state persistence for crash recovery.
//!
//! Each volume's state is serialized as JSON to `{state_dir}/{volume_id}.json`.
//! On startup the operator calls [`VolumeStateStore::load_all`] to recover
//! previously active volumes.

use std::collections::HashMap;
use std::path::PathBuf;

use libcsi::{VolumeId, VolumeState};
use serde::{Deserialize, Serialize};
use tracing::warn;

/// Persisted state for a single volume on this node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalVolumeState {
    pub volume_id: VolumeId,
    pub state: VolumeState,
    pub staging_target_path: Option<String>,
    pub target_path: Option<String>,
    /// Volume context from the original stage request, preserved for recovery.
    #[serde(default)]
    pub volume_context: HashMap<String, String>,
}

/// JSON-file-backed store living under a configurable directory.
pub struct VolumeStateStore {
    state_dir: PathBuf,
}

impl VolumeStateStore {
    /// Create a new store rooted at `state_dir`, creating it if absent.
    pub fn new(state_dir: impl Into<PathBuf>) -> std::io::Result<Self> {
        let state_dir = state_dir.into();
        std::fs::create_dir_all(&state_dir)?;
        Ok(Self { state_dir })
    }

    fn path_for(&self, volume_id: &VolumeId) -> PathBuf {
        self.state_dir.join(format!("{volume_id}.json"))
    }

    /// Persist volume state to disk.
    pub fn save(&self, state: &LocalVolumeState) -> std::io::Result<()> {
        let path = self.path_for(&state.volume_id);
        let json = serde_json::to_vec_pretty(state)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, json)
    }

    /// Load a single volume's state.
    pub fn load(&self, volume_id: &VolumeId) -> std::io::Result<LocalVolumeState> {
        let path = self.path_for(volume_id);
        let data = std::fs::read(path)?;
        serde_json::from_slice(&data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Remove persisted state for a volume.
    pub fn delete(&self, volume_id: &VolumeId) -> std::io::Result<()> {
        let path = self.path_for(volume_id);
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Load all persisted volume states (for startup recovery).
    pub fn load_all(&self) -> std::io::Result<Vec<LocalVolumeState>> {
        let entries = match std::fs::read_dir(&self.state_dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        let mut states = Vec::new();
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            match std::fs::read(&path).and_then(|data| {
                serde_json::from_slice::<LocalVolumeState>(&data)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
            }) {
                Ok(state) => states.push(state),
                Err(e) => {
                    warn!(
                        path = %path.display(),
                        error = %e,
                        "skipping corrupt volume state file"
                    );
                }
            }
        }
        Ok(states)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_load_delete_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let store = VolumeStateStore::new(tmp.path().join("state")).unwrap();

        let vol_id = VolumeId::from("vol-test-1");
        let state = LocalVolumeState {
            volume_id: vol_id.clone(),
            state: VolumeState::Staged,
            staging_target_path: Some("/mnt/staging".into()),
            target_path: None,
            volume_context: HashMap::from([("key".into(), "val".into())]),
        };

        store.save(&state).unwrap();

        let loaded = store.load(&vol_id).unwrap();
        assert_eq!(loaded.volume_id, vol_id);
        assert_eq!(loaded.state, VolumeState::Staged);
        assert_eq!(loaded.staging_target_path.as_deref(), Some("/mnt/staging"));
        assert!(loaded.target_path.is_none());
        assert_eq!(
            loaded.volume_context.get("key").map(|s| s.as_str()),
            Some("val")
        );

        store.delete(&vol_id).unwrap();
        assert!(store.load(&vol_id).is_err());
    }

    #[test]
    fn load_all_recovers_multiple() {
        let tmp = tempfile::tempdir().unwrap();
        let store = VolumeStateStore::new(tmp.path().join("state")).unwrap();

        for i in 0..3 {
            let state = LocalVolumeState {
                volume_id: VolumeId::from(format!("vol-{i}")),
                state: VolumeState::Published,
                staging_target_path: Some(format!("/staging/{i}")),
                target_path: Some(format!("/target/{i}")),
                volume_context: HashMap::new(),
            };
            store.save(&state).unwrap();
        }

        let all = store.load_all().unwrap();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn delete_nonexistent_is_ok() {
        let tmp = tempfile::tempdir().unwrap();
        let store = VolumeStateStore::new(tmp.path().join("state")).unwrap();
        assert!(store.delete(&VolumeId::from("nonexistent")).is_ok());
    }

    #[test]
    fn load_all_returns_empty_when_dir_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let store = VolumeStateStore {
            state_dir: tmp.path().join("nonexistent"),
        };
        let all = store.load_all().unwrap();
        assert!(all.is_empty());
    }
}
