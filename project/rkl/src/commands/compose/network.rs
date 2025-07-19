use crate::commands::compose::spec::ComposeSpec;
use anyhow::Ok;
use anyhow::Result;

pub struct NetworkManager {}

impl NetworkManager {
    pub fn new() -> Self {
        Self {}
    }

    pub fn handle(&self, _: &ComposeSpec) -> Result<()> {
        // let mut res = vec![];
        Ok(())
    }
}
