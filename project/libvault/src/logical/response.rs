use std::collections::HashMap;

use better_default::Default;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::{
    errors::RvError,
    logical::{Auth, secret::SecretData},
};

lazy_static! {
    static ref HTTP_RAW_BODY: &'static str = "http_raw_body";
    static ref HTTP_CONTENT_TYPE: &'static str = "http_content_type";
    static ref HTTP_STATUS_CODE: &'static str = "http_status_code";
}

#[derive(Debug, Eq, PartialEq, Clone, Default, Serialize, Deserialize)]
pub struct Response {
    #[serde(default)]
    pub request_id: String,
    #[serde(skip)]
    pub headers: Option<HashMap<String, String>>,
    pub data: Option<Map<String, Value>>,
    pub auth: Option<Auth>,
    pub secret: Option<SecretData>,
    pub redirect: String,
    // warnings allow operations or backends to return warnings in response
    // to user actions without failing the action outright.
    pub warnings: Vec<String>,
}

impl Response {
    pub fn new() -> Self {
        Self {
            ..Default::default()
        }
    }

    pub fn data_response(data: Option<Map<String, Value>>) -> Self {
        let mut resp = Response::new();
        resp.data = data;
        resp
    }

    pub fn list_response(keys: &[String]) -> Self {
        let value = serde_json::to_value(keys);
        let mut resp = Response::new();
        if value.is_ok() {
            resp.data = json!({
                "keys": value.unwrap(),
            })
            .as_object()
            .cloned();
        }
        resp
    }

    pub fn help_response(text: &str, see_also: &[String]) -> Self {
        let value = serde_json::to_value(see_also);
        let mut resp = Response::new();
        if value.is_ok() {
            resp.data = json!({
                "help": text.to_string(),
                "see_also": value.unwrap(),
            })
            .as_object()
            .cloned();
        }
        resp
    }

    pub fn error_response(text: &str) -> Self {
        let mut resp = Response::new();
        resp.data = json!({
            "error": text.to_string(),
        })
        .as_object()
        .cloned();
        resp
    }

    pub fn respond_with_status_code(resp: Option<Response>, code: u8) -> Self {
        let mut ret = Response::new();
        let mut data: Map<String, Value> = json!({
            HTTP_CONTENT_TYPE.to_string(): "application/json",
            HTTP_STATUS_CODE.to_string(): code,
        })
        .as_object()
        .unwrap()
        .clone();

        if let Some(response) = resp {
            let raw_body = serde_json::to_value(response).unwrap();
            data.insert(HTTP_RAW_BODY.to_string(), raw_body);
        }

        ret.data = Some(data);

        ret
    }

    pub fn add_warning(&mut self, warning: &str) {
        self.warnings.push(warning.to_string());
    }

    pub fn to_string(&self) -> Result<String, RvError> {
        Ok(serde_json::to_string(self)?)
    }

    pub fn set_request_id(&mut self, id: &str) {
        self.request_id = id.to_string()
    }
}
