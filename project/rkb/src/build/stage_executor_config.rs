use std::collections::HashMap;

use super::build_config::BuildConfig;

#[derive(Debug, Clone, Default)]
pub struct StageExecutorConfig {
    pub build_config: BuildConfig,
    pub global_args: HashMap<String, Option<String>>,
}

impl StageExecutorConfig {
    pub fn build_config(mut self, build_config: BuildConfig) -> Self {
        self.build_config = build_config;
        self
    }

    pub fn global_args(mut self, global_args: &HashMap<String, Option<String>>) -> Self {
        self.global_args = global_args.clone();
        self
    }
}