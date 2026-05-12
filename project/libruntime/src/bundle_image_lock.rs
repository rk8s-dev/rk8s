//! Serialize OCI `config.json` writes and container `create` for a shared image bundle directory.
//!
//! Multiple Pods can share the same pulled-image path; without a per-bundle lock, concurrent
//! writers overwrite each other's `config.json` before the runtime reads it.

use lazy_static::lazy_static;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

lazy_static! {
    static ref IMAGE_BUNDLE_LOCKS: Mutex<HashMap<String, Arc<Mutex<()>>>> =
        Mutex::new(HashMap::new());
}

fn bundle_lock_key(bundle_path: &Path) -> String {
    std::fs::canonicalize(bundle_path)
        .unwrap_or_else(|_| bundle_path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

/// Run `f` while holding an exclusive lock for this image bundle path (canonicalized when possible).
pub fn with_image_bundle_lock<R>(bundle_path: &Path, f: impl FnOnce() -> R) -> R {
    let key = bundle_lock_key(bundle_path);
    let lock = {
        let mut map = IMAGE_BUNDLE_LOCKS
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        map.entry(key)
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    };
    let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
    f()
}
