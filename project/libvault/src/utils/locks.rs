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
        let mut locks = Self { locks: Vec::with_capacity(LOCK_COUNT) };

        for _ in 0..LOCK_COUNT {
            locks.locks.push(Arc::new(LockEntry { lock: Arc::new(tokio::sync::RwLock::new(0)) }));
        }

        locks
    }

    pub fn get_lock(&self, key: &str) -> Arc<LockEntry> {
        let index: usize = blake2b256_hash(key)[0].into();
        self.locks[index].clone()
    }
}

#[cfg(test)]
mod test {
    use std::{sync::RwLock, thread::sleep, time::Duration};

    use super::*;

    struct MyTestData {
        lock: Locks,
        num: RwLock<u32>,
    }

    async fn write_case(data: Arc<MyTestData>) -> u32 {
        let lock_entry = data.lock.get_lock("test");
        let _locked = lock_entry.lock.write().await;
        sleep(Duration::from_secs(5));
        let mut num = data.num.write().unwrap();
        *num = *num * 2;
        *num
    }

    async fn read_case(data: Arc<MyTestData>) -> u32 {
        let lock_entry = data.lock.get_lock("test");
        let _locked = lock_entry.lock.read().await;
        let num = data.num.read().unwrap();
        *num
    }

    #[tokio::test]
    async fn test_locks_writer_reader() {
        let data = Arc::new(MyTestData { lock: Locks::new(), num: RwLock::new(11) });

        let data_writer = data.clone();
        let data_reader = data.clone();

        let writer = tokio::spawn(async {
            let num = write_case(data_writer).await;
            assert_eq!(num, 22);
        });

        sleep(Duration::from_secs(1));

        let reader = tokio::spawn(async {
            let num = read_case(data_reader).await;
            assert_eq!(num, 22);
        });

        writer.await.unwrap();
        sleep(Duration::from_secs(1));
        reader.await.unwrap();

        assert_eq!(*data.num.read().unwrap(), 22);
    }

    #[tokio::test]
    async fn test_locks_reader_writer() {
        let data = Arc::new(MyTestData { lock: Locks::new(), num: RwLock::new(11) });

        let data_writer = data.clone();
        let data_reader = data.clone();

        let reader = tokio::spawn(async {
            let num = read_case(data_reader).await;
            assert_eq!(num, 11);
        });

        sleep(Duration::from_secs(1));

        let writer = tokio::spawn(async {
            let num = write_case(data_writer).await;
            assert_eq!(num, 22);
        });

        reader.await.unwrap();
        writer.await.unwrap();

        assert_eq!(*data.num.read().unwrap(), 22);
    }

    #[tokio::test]
    async fn test_locks_writer_writer() {
        let data = Arc::new(MyTestData { lock: Locks::new(), num: RwLock::new(11) });

        let data_writer1 = data.clone();
        let data_writer2 = data.clone();

        let writer1 = tokio::spawn(async {
            let num = write_case(data_writer1).await;
            assert_eq!(num, 22);
        });

        sleep(Duration::from_secs(1));

        let writer2 = tokio::spawn(async {
            let num = write_case(data_writer2).await;
            assert_eq!(num, 44);
        });

        writer1.await.unwrap();
        writer2.await.unwrap();

        assert_eq!(*data.num.read().unwrap(), 44);
    }

    #[tokio::test]
    async fn test_locks_reader_reader() {
        let data = Arc::new(MyTestData { lock: Locks::new(), num: RwLock::new(11) });

        let data_reader1 = data.clone();
        let data_reader2 = data.clone();

        let reader1 = tokio::spawn(async {
            let num = read_case(data_reader1).await;
            assert_eq!(num, 11);
        });

        sleep(Duration::from_secs(1));

        let reader2 = tokio::spawn(async {
            let num = read_case(data_reader2).await;
            assert_eq!(num, 11);
        });

        reader1.await.unwrap();
        reader2.await.unwrap();
        assert_eq!(*data.num.read().unwrap(), 11);
    }
}
