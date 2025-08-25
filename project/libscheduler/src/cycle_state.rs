use std::{any::Any, collections::{HashMap, HashSet}, rc::Rc};

#[derive(Default)]
pub struct CycleState {
    storage: HashMap<String, Rc<dyn Any>>,
    skip_filter_plugins: HashSet<String>,
    skip_score_plugins: HashSet<String>,
    skip_pre_bind_plugins: HashSet<String>,
}

impl CycleState {
    pub fn skip_filter_plugins(&self) -> &HashSet<String> {
        &self.skip_filter_plugins
    }

    pub fn skip_score_plugins(&self) -> &HashSet<String> {
        &self.skip_score_plugins
    }

    pub fn skip_pre_bind_plugins(&self) -> &HashSet<String> {
        &self.skip_pre_bind_plugins
    }

    pub fn read<T: 'static>(&self, key: &str) -> Result<Rc<T>, Rc<dyn Any>> {
        let res = self.storage.get(key);
        if let Some(i) = res {
            i.clone().downcast()
        } else {
            Err(Rc::new(()))
        }
    }

    pub fn write(&mut self, key: &str, value: Rc<dyn Any>) {
        self.storage.insert(key.to_string(), value);
    }
}
