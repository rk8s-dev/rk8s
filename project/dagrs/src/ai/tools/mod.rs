use serde_json::Value;
use std::error::Error;

pub trait Tool: Send + Sync {
    fn name(&self) -> String;
    fn description(&self) -> String;
    fn call(&self, args: Value) -> Result<Value, Box<dyn Error + Send + Sync>>;
}

#[derive(Default)]
pub struct ToolSet {
    pub tools: Vec<Box<dyn Tool>>,
}
