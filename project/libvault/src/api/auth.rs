use serde_json::{Map, Value};

use super::{Client, HttpResponse};
use crate::errors::RvError;

pub trait LoginHandler: Send + Sync {
    fn auth(&self, client: &Client, data: &Map<String, Value>) -> Result<HttpResponse, RvError>;
    fn help(&self) -> String;
}
