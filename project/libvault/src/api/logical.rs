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
    pub async fn read(&self, path: &str) -> Result<HttpResponse, RvError> {
        self.client
            .request_raw::<_, Value>("GET", format!("/v1/{path}"), None::<Value>)
            .await
    }

    pub async fn write(
        &self,
        path: &str,
        data: Option<Map<String, Value>>,
    ) -> Result<HttpResponse, RvError> {
        self.client
            .request_raw("POST", format!("/v1/{path}"), data)
            .await
    }

    pub async fn list(&self, path: &str) -> Result<HttpResponse, RvError> {
        let mut ret = self
            .client
            .request_raw::<_, Value>("LIST", format!("/v1/{path}"), None::<Value>)
            .await?;
        if ret.response_status != 200 || ret.response_data.is_none() {
            return Ok(ret);
        }

        let data = ret.response_data.unwrap();
        ret.response_data = Some(data["data"].clone());

        Ok(ret)
    }

    pub async fn delete(
        &self,
        path: &str,
        data: Option<Map<String, Value>>,
    ) -> Result<HttpResponse, RvError> {
        self.client
            .request_raw("DELETE", format!("/v1/{path}"), data)
            .await
    }
}
