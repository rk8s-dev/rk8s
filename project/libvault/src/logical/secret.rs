#[cfg(not(feature = "sync_handler"))]
use std::{future::Future, pin::Pin};
use std::{sync::Arc, time::Duration};

use derive_more::{Deref, DerefMut};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::{Backend, Request, Response, lease::Lease};
use crate::errors::RvError;

#[cfg(not(feature = "sync_handler"))]
type SecretOperationHandler = dyn for<'a> Fn(
        &'a dyn Backend,
        &'a mut Request,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Response>, RvError>> + Send + 'a>>
    + Send
    + Sync;
#[cfg(feature = "sync_handler")]
type SecretOperationHandler =
    dyn Fn(&dyn Backend, &mut Request) -> Result<Option<Response>, RvError> + Send + Sync;

#[derive(Debug, Clone, Eq, Default, PartialEq, Serialize, Deserialize, Deref, DerefMut)]
pub struct SecretData {
    #[deref]
    #[deref_mut]
    #[serde(flatten)]
    pub lease: Lease,
    pub lease_id: String,
    #[serde(default)]
    pub internal_data: Map<String, Value>,
}

#[derive(Clone)]
pub struct Secret {
    pub secret_type: String,
    pub default_duration: Duration,
    pub renew_handler: Option<Arc<SecretOperationHandler>>,
    pub revoke_handler: Option<Arc<SecretOperationHandler>>,
}

impl Secret {
    pub fn new() -> Self {
        Self {
            secret_type: String::new(),
            default_duration: Duration::from_secs(0),
            renew_handler: None,
            revoke_handler: None,
        }
    }

    pub fn builder() -> SecretBuilder {
        SecretBuilder::new()
    }
}

impl Default for Secret {
    fn default() -> Self {
        Self::new()
    }
}

#[maybe_async::maybe_async]
impl Secret {
    pub fn renewable(&self) -> bool {
        self.renew_handler.is_some()
    }

    pub fn response(
        &self,
        data: Option<Map<String, Value>>,
        internal: Option<Map<String, Value>>,
    ) -> Response {
        let mut lease = Lease::default();
        lease.ttl = self.default_duration;
        lease.renewable = self.renewable();

        let mut secret = SecretData {
            lease,
            lease_id: String::new(),
            internal_data: Map::new(),
        };

        if internal.is_some() {
            secret.internal_data.clone_from(internal.as_ref().unwrap());
        }

        secret.internal_data.insert(
            "secret_type".to_owned(),
            Value::String(self.secret_type.clone()),
        );

        let mut resp = Response::default();
        resp.data = data;
        resp.secret = Some(secret);
        resp
    }

    pub async fn renew(
        &self,
        backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        if !self.renewable() || self.renew_handler.is_none() {
            return Err(RvError::ErrLogicalOperationUnsupported);
        }

        (self.renew_handler.as_ref().unwrap())(backend, req).await
    }

    pub async fn revoke(
        &self,
        backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        if self.revoke_handler.is_none() {
            return Err(RvError::ErrLogicalOperationUnsupported);
        }

        (self.revoke_handler.as_ref().unwrap())(backend, req).await
    }
}

#[derive(Default, Clone)]
pub struct SecretBuilder {
    secret: Secret,
}

impl SecretBuilder {
    pub fn new() -> Self {
        Self {
            secret: Secret::new(),
        }
    }

    pub fn secret_type(mut self, secret_type: impl Into<String>) -> Self {
        self.secret.secret_type = secret_type.into();
        self
    }

    pub fn default_duration(mut self, duration: Duration) -> Self {
        self.secret.default_duration = duration;
        self
    }

    pub fn default_duration_secs(mut self, duration_secs: u64) -> Self {
        self.secret.default_duration = Duration::from_secs(duration_secs);
        self
    }

    #[cfg(not(feature = "sync_handler"))]
    pub fn renew_handler<H>(mut self, handler: H) -> Self
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
        self.secret.renew_handler = Some(Arc::new(handler));
        self
    }

    #[cfg(feature = "sync_handler")]
    pub fn renew_handler<H>(mut self, handler: H) -> Self
    where
        H: Fn(&dyn Backend, &mut Request) -> Result<Option<Response>, RvError>
            + Send
            + Sync
            + 'static,
    {
        self.secret
            .renew_handler
            .replace(Arc::new(move |backend, req| handler(backend, req)));
        self
    }

    #[cfg(not(feature = "sync_handler"))]
    pub fn revoke_handler<H>(mut self, handler: H) -> Self
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
        self.secret.revoke_handler = Some(Arc::new(handler));
        self
    }

    #[cfg(feature = "sync_handler")]
    pub fn revoke_handler<H>(mut self, handler: H) -> Self
    where
        H: Fn(&dyn Backend, &mut Request) -> Result<Option<Response>, RvError>
            + Send
            + Sync
            + 'static,
    {
        self.secret
            .revoke_handler
            .replace(Arc::new(move |backend, req| handler(backend, req)));
        self
    }

    pub fn build(self) -> Secret {
        self.secret
    }

    pub fn build_arc(self) -> Arc<Secret> {
        Arc::new(self.secret)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    struct MyTest;

    #[maybe_async::maybe_async]
    impl MyTest {
        pub fn new() -> Self {
            MyTest
        }

        pub async fn noop(
            &self,
            _backend: &dyn Backend,
            _req: &mut Request,
        ) -> Result<Option<Response>, RvError> {
            Ok(None)
        }
    }

    #[maybe_async::maybe_async]
    pub async fn noop(
        _backend: &dyn Backend,
        _req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        Ok(None)
    }

    #[test]
    fn test_logical_secret() {
        let t = Arc::new(MyTest::new());

        #[cfg(not(feature = "sync_handler"))]
        let secret = {
            let renew = Arc::clone(&t);

            Secret::builder()
                .secret_type("kv")
                .default_duration_secs(60)
                .renew_handler(move |backend, req| {
                    let renew = Arc::clone(&renew);
                    Box::pin(async move { renew.noop(backend, req).await })
                })
                .revoke_handler(|backend, req| Box::pin(async move { noop(backend, req).await }))
                .build()
        };

        #[cfg(feature = "sync_handler")]
        let secret = {
            let renew = Arc::clone(&t);

            Secret::builder()
                .secret_type("kv")
                .default_duration_secs(60)
                .renew_handler(move |backend, req| renew.noop(backend, req))
                .revoke_handler(noop)
                .build()
        };

        assert_eq!(&secret.secret_type, "kv");
        assert_eq!(secret.default_duration, Duration::from_secs(60));
        assert!(secret.renew_handler.is_some());
        assert!(secret.revoke_handler.is_some());
    }
}
