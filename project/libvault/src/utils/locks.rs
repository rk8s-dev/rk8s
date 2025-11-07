//! This module is a Rust replica of
//! https://github.com/hashicorp/vault/blob/main/sdk/helper/locksutil/locks.go

use std::sync::Arc;

use super::crypto::blake2b256_hash;

static LOCK_COUNT: usize = 256;

#[derive(Debug)]
pub struct LockEntry {
    pub lock: Arc<tokio::sync::RwLock<u8>>,
}

#[derive(Debug)]
pub struct Locks {
    pub locks: Vec<Arc<LockEntry>>,
}

impl Locks {
    pub fn new() -> Self {
        let mut locks = Self {
            locks: Vec::with_capacity(LOCK_COUNT),
        };

        for _ in 0..LOCK_COUNT {
            locks.locks.push(Arc::new(LockEntry {
                lock: Arc::new(tokio::sync::RwLock::new(0)),
            }));
        }

        locks
    }

    pub fn get_lock(&self, key: &str) -> Arc<LockEntry> {
        let index: usize = blake2b256_hash(key)[0].into();
        self.locks[index].clone()
    }
}
