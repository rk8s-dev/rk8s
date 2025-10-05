use async_trait::async_trait;
use serde_json::{Map, Value};

use super::{Client, HttpResponse};
use crate::errors::RvError;

#[async_trait]
pub trait LoginHandler: Send + Sync {
    async fn auth(
        &self,
        client: &Client,
        data: &Map<String, Value>,
    ) -> Result<HttpResponse, RvError>;
    fn help(&self) -> String;
}
