use std::default::Default;

use crate::{
    errors::RvError,
    storage::{Backend, BackendEntry},
};

#[derive(Default)]
pub struct MockBackend(());

#[async_trait::async_trait]
impl Backend for MockBackend {
    async fn list(&self, _prefix: &str) -> Result<Vec<String>, RvError> {
        Ok(Vec::new())
    }

    async fn get(&self, _k: &str) -> Result<Option<BackendEntry>, RvError> {
        Ok(None)
    }

    async fn put(&self, _entry: &BackendEntry) -> Result<(), RvError> {
        Ok(())
    }

    async fn delete(&self, _k: &str) -> Result<(), RvError> {
        Ok(())
    }
}

impl MockBackend {
    pub fn new() -> Self {
        MockBackend(())
    }
}
