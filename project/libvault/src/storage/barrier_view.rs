use std::{any::Any, sync::Arc};

use super::{barrier::SecurityBarrier, Storage, StorageEntry};
use crate::errors::RvError;

pub struct BarrierView {
    barrier: Arc<dyn SecurityBarrier>,
    prefix: String,
}

#[maybe_async::maybe_async]
impl Storage for BarrierView {
    async fn list(&self, prefix: &str) -> Result<Vec<String>, RvError> {
        self.sanity_check(prefix)?;
        self.barrier.list(self.expand_key(prefix).as_str()).await
    }

    async fn get(&self, key: &str) -> Result<Option<StorageEntry>, RvError> {
        self.sanity_check(key)?;
        let storage_entry = self.barrier.get(self.expand_key(key).as_str()).await?;
        if let Some(entry) = storage_entry {
            Ok(Some(StorageEntry { key: self.truncate_key(entry.key.as_str()), value: entry.value }))
        } else {
            Ok(None)
        }
    }

    async fn put(&self, entry: &StorageEntry) -> Result<(), RvError> {
        self.sanity_check(entry.key.as_str())?;
        let nested = StorageEntry { key: self.expand_key(entry.key.as_str()), value: entry.value.clone() };
        self.barrier.put(&nested).await
    }

    async fn delete(&self, key: &str) -> Result<(), RvError> {
        self.sanity_check(key)?;
        self.barrier.delete(self.expand_key(key).as_str()).await
    }

    async fn lock(&self, lock_name: &str) -> Result<Box<dyn Any>, RvError> {
        self.barrier.lock(lock_name).await
    }
}

#[maybe_async::maybe_async]
impl BarrierView {
    pub fn new(barrier: Arc<dyn SecurityBarrier>, prefix: &str) -> Self {
        Self { barrier, prefix: prefix.to_string() }
    }

    pub fn new_sub_view(&self, prefix: &str) -> Self {
        Self { barrier: self.barrier.clone(), prefix: self.expand_key(prefix) }
    }

    pub async fn get_keys(&self) -> Result<Vec<String>, RvError> {
        let mut paths = vec!["".to_string()];
        let mut keys = Vec::new();
        while !paths.is_empty() {
            let n = paths.len();
            let curr = paths[n - 1].to_owned();
            paths.pop();

            let items = self.list(curr.as_str()).await?;
            for p in items {
                let path = format!("{curr}{p}");
                if p.ends_with('/') {
                    paths.push(path);
                } else {
                    keys.push(path.to_owned());
                }
            }
        }
        keys.sort();
        Ok(keys)
    }

    pub async fn clear(&self) -> Result<(), RvError> {
        let keys = self.get_keys().await?;
        for key in keys {
            self.delete(key.as_str()).await?
        }
        Ok(())
    }

    pub fn as_storage(&self) -> &dyn Storage {
        self
    }

    fn sanity_check(&self, key: &str) -> Result<(), RvError> {
        if key.contains("..") || key.starts_with('/') {
            Err(RvError::ErrBarrierKeySanityCheckFailed)
        } else {
            Ok(())
        }
    }

    fn expand_key(&self, suffix: &str) -> String {
        format!("{}{}", self.prefix, suffix)
    }

    fn truncate_key(&self, full: &str) -> String {
        if let Some(result) = full.strip_prefix(self.prefix.as_str()) {
            result.to_string()
        } else {
            full.to_string()
        }
    }
}

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use rand::{thread_rng, Rng};

    use super::{super::*, *};
    use crate::test_utils::new_test_backend;

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_new_barrier_view() {
        let backend = new_test_backend("test_new_barrier_view");

        let mut key = vec![0u8; 32];
        thread_rng().fill(key.as_mut_slice());

        let aes_gcm_view = barrier_aes_gcm::AESGCMBarrier::new(backend.clone());

        let init = aes_gcm_view.init(key.as_slice()).await;
        assert!(init.is_ok());

        let view = barrier_view::BarrierView::new(Arc::new(aes_gcm_view), "test");
        assert_eq!(view.expand_key("foo"), "testfoo");
        assert!(view.sanity_check("foo").is_ok());
        assert!(view.sanity_check("../foo").is_err());
        assert!(view.sanity_check("foo/../").is_err());
    }
}
