//! The `libvault::context` module is intent to provide a generic key value storage.
//! This module is currently not used by any other part of `crate::libvault`.

use std::{
    any::Any,
    sync::{Arc, Mutex, RwLock},
};

use dashmap::DashMap;
use tokio::task::JoinHandle;

use crate::errors::RvError;

#[derive(Default, Debug)]
pub struct Context {
    tasks: Mutex<Vec<JoinHandle<()>>>,
    data_map: DashMap<String, Arc<dyn Any + Send + Sync>>,
    data_map_mut: DashMap<String, Arc<RwLock<dyn Any + Send + Sync>>>,
}

impl Context {
    pub fn new() -> Self {
        Self {
            data_map: DashMap::new(),
            data_map_mut: DashMap::new(),
            ..Default::default()
        }
    }

    pub fn set_mut(&self, key: &str, data: Arc<RwLock<dyn Any + Send + Sync>>) {
        self.data_map_mut.insert(key.to_string(), data);
    }

    pub fn get_mut(&self, key: &str) -> Option<Arc<RwLock<dyn Any + Send + Sync>>> {
        self.data_map_mut.get(key).map(|r| r.value().clone())
    }

    pub fn set(&self, key: &str, data: Arc<dyn Any + Send + Sync>) {
        self.data_map.insert(key.to_string(), data);
    }

    pub fn get(&self, key: &str) -> Option<Arc<dyn Any + Send + Sync>> {
        self.data_map.get(key).map(|r| r.clone())
    }

    pub fn add_task(&self, task: JoinHandle<()>) {
        let mut tasks = self.tasks.lock().unwrap();
        tasks.push(task)
    }

    pub fn clear_task(&self) {
        let mut tasks = self.tasks.lock().unwrap();
        tasks.clear()
    }

    pub async fn wait_task_finish(&self) -> Result<(), RvError> {
        let mut tasks = self.tasks.lock().unwrap();
        for task in tasks.iter_mut() {
            task.await?;
        }

        Ok(())
    }
}
