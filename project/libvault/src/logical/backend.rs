use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc};

use regex::Regex;
use serde_json::{Map, Value};

use super::{
    Backend, FieldType, Operation,
    path::{Path, PathBuilder},
    request::Request,
    response::Response,
    secret::{Secret, SecretBuilder},
};
use crate::{context::Context, errors::RvError};

type BackendOperationHandler = dyn for<'a> Fn(
        &'a dyn Backend,
        &'a mut Request,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Response>, RvError>> + Send + 'a>>
    + Send
    + Sync;

pub const CTX_KEY_BACKEND_PATH: &str = "backend.path";

#[derive(Clone)]
pub struct LogicalBackend {
    pub paths: Vec<Arc<Path>>,
    pub paths_re: Vec<Regex>,
    pub root_paths: Arc<Vec<String>>,
    pub unauth_paths: Arc<Vec<String>>,
    pub help: String,
    pub secrets: Vec<Arc<Secret>>,
    pub auth_renew_handler: Option<Arc<BackendOperationHandler>>,
    pub ctx: Arc<Context>,
}

#[async_trait::async_trait]
impl Backend for LogicalBackend {
    fn init(&mut self) -> Result<(), RvError> {
        if self.paths.len() == self.paths_re.len() {
            return Ok(());
        }

        for path in &self.paths {
            let mut pattern = path.pattern.clone();
            if !path.pattern.starts_with('^') {
                pattern = format!("^{}", &pattern);
            }

            if !path.pattern.ends_with('$') {
                pattern = format!("{}$", &pattern);
            }

            let re = Regex::new(&pattern)?;
            self.paths_re.push(re);
        }

        Ok(())
    }

    fn setup(&self, _key: &str) -> Result<(), RvError> {
        Ok(())
    }

    fn cleanup(&self) -> Result<(), RvError> {
        Ok(())
    }

    fn get_unauth_paths(&self) -> Option<Arc<Vec<String>>> {
        Some(self.unauth_paths.clone())
    }

    fn get_root_paths(&self) -> Option<Arc<Vec<String>>> {
        Some(self.root_paths.clone())
    }

    fn get_ctx(&self) -> Option<Arc<Context>> {
        Some(self.ctx.clone())
    }

    async fn handle_request(&self, req: &mut Request) -> Result<Option<Response>, RvError> {
        if req.storage.is_none() {
            return Err(RvError::ErrRequestNotReady);
        }

        match req.operation {
            Operation::Renew | Operation::Revoke => {
                return self.handle_revoke_renew(req).await;
            }
            _ => {}
        }

        if req.path.is_empty() && req.operation == Operation::Help {
            return self.handle_root_help(req).await;
        }

        if let Some((path, captures)) = self.match_path(&req.path) {
            if !captures.is_empty() {
                let mut data = Map::new();
                captures.iter().for_each(|(key, value)| {
                    data.insert(key.to_string(), Value::String(value.to_string()));
                });
                req.data = Some(data);
            }

            req.match_path = Some(path.clone());
            for operation in &path.operations {
                if operation.op == req.operation {
                    self.ctx.set(CTX_KEY_BACKEND_PATH, path.clone());
                    let ret = operation.handle_request(self, req).await;
                    self.clear_secret_field(req);
                    return ret;
                }
            }

            return Err(RvError::ErrLogicalOperationUnsupported);
        }

        Err(RvError::ErrLogicalPathUnsupported)
    }

    fn secret(&self, key: &str) -> Option<&Arc<Secret>> {
        self.secrets.iter().find(|s| s.secret_type == key)
    }
}

impl LogicalBackend {
    pub fn new() -> Self {
        Self {
            paths: Vec::new(),
            paths_re: Vec::new(),
            root_paths: Arc::new(Vec::new()),
            unauth_paths: Arc::new(Vec::new()),
            help: String::new(),
            secrets: Vec::new(),
            auth_renew_handler: None,
            ctx: Arc::new(Context::new()),
        }
    }

    pub fn builder() -> LogicalBackendBuilder {
        LogicalBackendBuilder::new()
    }

    pub async fn handle_auth_renew(&self, req: &mut Request) -> Result<Option<Response>, RvError> {
        let Some(auth_renew_handler) = self.auth_renew_handler.as_ref() else {
            log::error!("this auth type doesn't support renew");
            return Err(RvError::ErrLogicalOperationUnsupported);
        };

        auth_renew_handler(self, req).await
    }

    pub async fn handle_revoke_renew(
        &self,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        if req.operation == Operation::Renew && req.auth.is_some() {
            return self.handle_auth_renew(req).await;
        }

        if req.secret.is_none() {
            log::error!("request has no secret");
            return Ok(None);
        }

        if let Some(raw_secret_type) = req
            .secret
            .as_ref()
            .unwrap()
            .internal_data
            .get("secret_type")
            && let Some(secret_type) = raw_secret_type.as_str()
            && let Some(secret) = self.secret(secret_type)
        {
            match req.operation {
                Operation::Renew => {
                    return secret.renew(self, req).await;
                }
                Operation::Revoke => {
                    return secret.revoke(self, req).await;
                }
                _ => {
                    log::error!("invalid operation for revoke/renew: {}", req.operation);
                    return Ok(None);
                }
            }
        }

        log::error!("secret is unsupported by this backend");
        Ok(None)
    }

    pub async fn handle_root_help(&self, _req: &mut Request) -> Result<Option<Response>, RvError> {
        Ok(None)
    }

    pub fn match_path(&self, path: &str) -> Option<(Arc<Path>, HashMap<String, String>)> {
        for (i, re) in self.paths_re.iter().enumerate() {
            if let Some(matches) = re.captures(path) {
                let mut captures = HashMap::new();
                let path = self.paths[i].clone();
                for (i, name) in re.capture_names().enumerate() {
                    if let Some(name) = name {
                        captures.insert(name.to_string(), matches[i].to_string());
                    }
                }

                return Some((path, captures));
            }
        }

        None
    }

    fn clear_secret_field(&self, req: &mut Request) {
        for path in &self.paths {
            for (key, field) in &path.fields {
                if field.field_type == FieldType::SecretStr {
                    req.clear_data(key);
                }
            }
        }
    }
}

pub trait IntoPathArc {
    fn into_path_arc(self) -> Arc<Path>;
}

impl IntoPathArc for Arc<Path> {
    fn into_path_arc(self) -> Arc<Path> {
        self
    }
}

impl IntoPathArc for Path {
    fn into_path_arc(self) -> Arc<Path> {
        Arc::new(self)
    }
}

impl IntoPathArc for PathBuilder {
    fn into_path_arc(self) -> Arc<Path> {
        Arc::new(self.build())
    }
}

pub trait IntoSecretArc {
    fn into_secret_arc(self) -> Arc<Secret>;
}

impl IntoSecretArc for Arc<Secret> {
    fn into_secret_arc(self) -> Arc<Secret> {
        self
    }
}

impl IntoSecretArc for Secret {
    fn into_secret_arc(self) -> Arc<Secret> {
        Arc::new(self)
    }
}

impl IntoSecretArc for SecretBuilder {
    fn into_secret_arc(self) -> Arc<Secret> {
        self.build_arc()
    }
}

#[derive(Clone)]
pub struct LogicalBackendBuilder {
    backend: LogicalBackend,
}

impl Default for LogicalBackendBuilder {
    fn default() -> Self {
        Self {
            backend: LogicalBackend::new(),
        }
    }
}

impl LogicalBackendBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn context(mut self, ctx: Arc<Context>) -> Self {
        self.backend.ctx = ctx;
        self
    }

    pub fn help(mut self, help: impl Into<String>) -> Self {
        self.backend.help = help.into();
        self
    }

    pub fn path<P>(mut self, path: P) -> Self
    where
        P: IntoPathArc,
    {
        self.backend.paths.push(path.into_path_arc());
        self
    }

    pub fn paths<I, P>(mut self, paths: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: IntoPathArc,
    {
        self.backend
            .paths
            .extend(paths.into_iter().map(|p| p.into_path_arc()));
        self
    }

    pub fn secret<S>(mut self, secret: S) -> Self
    where
        S: IntoSecretArc,
    {
        self.backend.secrets.push(secret.into_secret_arc());
        self
    }

    pub fn secrets<I, S>(mut self, secrets: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: IntoSecretArc,
    {
        self.backend
            .secrets
            .extend(secrets.into_iter().map(|s| s.into_secret_arc()));
        self
    }

    pub fn unauth_paths<I, S>(mut self, paths: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.backend.unauth_paths = Arc::new(paths.into_iter().map(Into::into).collect());
        self
    }

    pub fn add_unauth_path(mut self, path: impl Into<String>) -> Self {
        Arc::make_mut(&mut self.backend.unauth_paths).push(path.into());
        self
    }

    pub fn root_paths<I, S>(mut self, paths: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.backend.root_paths = Arc::new(paths.into_iter().map(Into::into).collect());
        self
    }

    pub fn add_root_path(mut self, path: impl Into<String>) -> Self {
        Arc::make_mut(&mut self.backend.root_paths).push(path.into());
        self
    }

    pub fn auth_renew_handler<H>(mut self, handler: H) -> Self
    where
        H: for<'a> Fn(
                &'a dyn Backend,
                &'a mut Request,
            ) -> Pin<
                Box<dyn Future<Output = Result<Option<Response>, RvError>> + Send + 'a>,
            > + Send
            + Sync
            + 'static,
    {
        self.backend.auth_renew_handler = Some(Arc::new(handler));
        self
    }

    pub fn build(self) -> LogicalBackend {
        self.backend
    }

    pub fn build_arc(self) -> Arc<LogicalBackend> {
        Arc::new(self.backend)
    }
}
