use std::{collections::HashMap, sync::Arc};

use better_default::Default;
use serde_json::{Map, Value};

use super::{Operation, Path};
use crate::{
    context::Context,
    errors::RvError,
    handler::{HandlePhase, Handler},
    logical::{auth::Auth, connection::Connection, secret::SecretData},
    storage::{Storage, StorageEntry},
};

#[derive(Default, Clone)]
pub struct Request {
    pub id: String,
    pub name: String,
    #[default(Operation::Read)]
    pub operation: Operation,
    pub path: String,
    pub match_path: Option<Arc<Path>>,
    pub headers: Option<HashMap<String, String>>,
    pub body: Option<Map<String, Value>>,
    pub data: Option<Map<String, Value>>,
    pub client_token: String,
    pub storage: Option<Arc<dyn Storage>>,
    pub connection: Option<Connection>,
    pub secret: Option<SecretData>,
    pub auth: Option<Auth>,
    pub handler: Option<Arc<dyn Handler>>,
    #[default(HandlePhase::PreRoute)]
    pub handle_phase: HandlePhase,
    #[default(Arc::new(Context::new()))]
    pub ctx: Arc<Context>,
}

impl Request {
    pub fn new<S: Into<String>>(path: S) -> Self {
        Self {
            path: path.into(),
            ..Default::default()
        }
    }

    pub fn new_read_request<S: Into<String>>(path: S) -> Self {
        Self {
            operation: Operation::Read,
            path: path.into(),
            ..Default::default()
        }
    }

    pub fn new_write_request<S: Into<String>>(path: S, body: Option<Map<String, Value>>) -> Self {
        Self {
            operation: Operation::Write,
            path: path.into(),
            body,
            ..Default::default()
        }
    }

    pub fn new_delete_request<S: Into<String>>(path: S, body: Option<Map<String, Value>>) -> Self {
        Self {
            operation: Operation::Delete,
            path: path.into(),
            body,
            ..Default::default()
        }
    }

    pub fn new_list_request<S: Into<String>>(path: S) -> Self {
        Self {
            operation: Operation::List,
            path: path.into(),
            ..Default::default()
        }
    }

    pub fn new_revoke_request<S: Into<String>>(
        path: S,
        secret: Option<SecretData>,
        data: Option<Map<String, Value>>,
    ) -> Self {
        Self {
            operation: Operation::Revoke,
            path: path.into(),
            secret,
            data,
            ..Default::default()
        }
    }

    pub fn new_renew_request<S: Into<String>>(
        path: S,
        secret: Option<SecretData>,
        data: Option<Map<String, Value>>,
    ) -> Self {
        Self {
            operation: Operation::Renew,
            path: path.into(),
            secret,
            data,
            ..Default::default()
        }
    }

    pub fn new_renew_auth_request<S: Into<String>>(
        path: S,
        auth: Option<Auth>,
        data: Option<Map<String, Value>>,
    ) -> Self {
        Self {
            operation: Operation::Renew,
            path: path.into(),
            auth,
            data,
            ..Default::default()
        }
    }

    pub fn bind_handler(&mut self, handler: Arc<dyn Handler>) {
        self.handler = Some(handler);
    }

    pub fn get_handler(&self) -> Option<Arc<dyn Handler>> {
        self.handler.clone()
    }

    fn get_data_raw(&self, key: &str, default: bool) -> Result<Value, RvError> {
        let Some(match_path) = self.match_path.as_ref() else {
            return Err(RvError::ErrRequestNotReady);
        };
        let Some(field) = match_path.get_field(key) else {
            return Err(RvError::ErrRequestNoDataField);
        };

        if let Some(data) = self.data.as_ref()
            && let Some(value) = data.get(key)
        {
            if !field.check_data_type(value) {
                return Err(RvError::ErrRequestFieldInvalid);
            }
            return Ok(value.clone());
        }

        if let Some(body) = self.body.as_ref()
            && let Some(value) = body.get(key)
        {
            if !field.check_data_type(value) {
                return Err(RvError::ErrRequestFieldInvalid);
            }
            return Ok(value.clone());
        }

        if default {
            if field.required {
                return Err(RvError::ErrRequestFieldNotFound);
            }

            return field.get_default();
        }

        Err(RvError::ErrRequestFieldNotFound)
    }

    pub fn get_data(&self, key: &str) -> Result<Value, RvError> {
        if self.match_path.is_none() {
            return Err(RvError::ErrRequestNotReady);
        }

        if self.data.is_none() && self.body.is_none() {
            return Err(RvError::ErrRequestNoData);
        }

        self.get_data_raw(key, false)
    }

    pub fn get_data_or_default(&self, key: &str) -> Result<Value, RvError> {
        if self.match_path.is_none() {
            return Err(RvError::ErrRequestNotReady);
        }

        if self.data.is_none() && self.body.is_none() {
            return Err(RvError::ErrRequestNoData);
        }

        self.get_data_raw(key, true)
    }

    pub fn get_data_or_next(&self, keys: &[&str]) -> Result<Value, RvError> {
        if self.match_path.is_none() {
            return Err(RvError::ErrRequestNotReady);
        }

        if self.data.is_none() && self.body.is_none() {
            return Err(RvError::ErrRequestNoData);
        }

        for &key in keys.iter() {
            match self.get_data_raw(key, false) {
                Ok(raw) => {
                    return Ok(raw);
                }
                Err(e) => {
                    if e != RvError::ErrRequestFieldNotFound {
                        return Err(e);
                    }
                }
            }
        }

        Err(RvError::ErrRequestFieldNotFound)
    }

    pub fn get_data_as_str(&self, key: &str) -> Result<String, RvError> {
        self.get_data(key)?
            .as_str()
            .ok_or(RvError::ErrRequestFieldInvalid)
            .and_then(|s| {
                if s.trim().is_empty() {
                    Err(RvError::ErrResponse(format!("missing {key}")))
                } else {
                    Ok(s.trim().to_string())
                }
            })
    }

    pub fn get_field_default_or_zero(&self, key: &str) -> Result<Value, RvError> {
        let Some(match_path) = self.match_path.as_ref() else {
            return Err(RvError::ErrRequestNotReady);
        };
        let Some(field) = match_path.get_field(key) else {
            return Err(RvError::ErrRequestNoDataField);
        };
        field.get_default()
    }

    pub fn data_iter(&self) -> impl Iterator<Item = (&String, &Value)> {
        let data_iter = self.data.as_ref().into_iter().flat_map(|m| m.iter());
        let body_iter = self.body.as_ref().into_iter().flat_map(|m| m.iter());
        data_iter.chain(body_iter)
    }

    //TODO: the sensitive data is still in the memory. Need to totally resolve this in `serde_json` someday.
    pub fn clear_data(&mut self, key: &str) {
        if let Some(data) = self.data.as_mut()
            && let Some(secret_str) = data.get_mut(key)
            && let Value::String(ref mut s) = *secret_str
        {
            "".clone_into(s);
        }

        if let Some(body) = self.body.as_mut()
            && let Some(secret_str) = body.get_mut(key)
            && let Value::String(ref mut s) = *secret_str
        {
            "".clone_into(s);
        }
    }

    pub async fn storage_list(&self, prefix: &str) -> Result<Vec<String>, RvError> {
        let Some(storage) = self.storage.as_ref() else {
            return Err(RvError::ErrRequestNotReady);
        };

        storage.list(prefix).await
    }

    pub async fn storage_get(&self, key: &str) -> Result<Option<StorageEntry>, RvError> {
        let Some(storage) = self.storage.as_ref() else {
            return Err(RvError::ErrRequestNotReady);
        };

        storage.get(key).await
    }

    pub async fn storage_put(&self, entry: &StorageEntry) -> Result<(), RvError> {
        let Some(storage) = self.storage.as_ref() else {
            return Err(RvError::ErrRequestNotReady);
        };

        storage.put(entry).await
    }

    pub async fn storage_delete(&self, key: &str) -> Result<(), RvError> {
        let Some(storage) = self.storage.as_ref() else {
            return Err(RvError::ErrRequestNotReady);
        };

        storage.delete(key).await
    }
}
