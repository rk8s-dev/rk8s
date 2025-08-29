use std::{
    any::Any,
    collections::{HashMap, HashSet},
};

#[derive(Default)]
pub struct CycleState {
    storage: HashMap<String, Box<dyn Any + Send + Sync>>,
    pub skip_filter_plugins: HashSet<String>,
    pub skip_score_plugins: HashSet<String>,
    pub _skip_pre_bind_plugins: HashSet<String>,
}

impl CycleState {
    pub fn read<T: 'static>(&self, key: &str) -> Option<&T> {
        let res = self.storage.get(key);
        if let Some(i) = res {
            i.downcast_ref()
        } else {
            None
        }
    }

    pub fn write(&mut self, key: &str, value: Box<dyn Any + Send + Sync>) {
        self.storage.insert(key.to_string(), value);
    }
}
