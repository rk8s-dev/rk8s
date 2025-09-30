use derive_more::Deref;
use serde_json::{Map, Value};

use super::{Client, HttpResponse};
use crate::errors::RvError;

#[derive(Deref)]
pub struct Logical<'a> {
    #[deref]
    pub client: &'a Client,
}

impl Client {
    pub fn logical(&self) -> Logical<'_> {
        Logical { client: self }
    }
}

impl Logical<'_> {
    pub fn read(&self, path: &str) -> Result<HttpResponse, RvError> {
        self.request_read(format!("/v1/{path}"))
    }

    pub fn write(&self, path: &str, data: Option<Map<String, Value>>) -> Result<HttpResponse, RvError> {
        self.request_write(format!("/v1/{path}"), data)
    }

    pub fn list(&self, path: &str) -> Result<HttpResponse, RvError> {
        let mut ret = self.request_list(format!("/v1/{path}"))?;
        if ret.response_status != 200 || ret.response_data.is_none() {
            return Ok(ret);
        }

        let data = ret.response_data.unwrap();
        ret.response_data = Some(data["data"].clone());

        Ok(ret)
    }

    pub fn delete(&self, path: &str, data: Option<Map<String, Value>>) -> Result<HttpResponse, RvError> {
        self.request_delete(format!("/v1/{path}"), data)
    }
}
