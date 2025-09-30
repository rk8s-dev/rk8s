#[cfg(not(feature = "sync_handler"))]
use std::{future::Future, pin::Pin};
use std::{sync::Arc, time::Duration};

use derive_more::{Deref, DerefMut};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::{lease::Lease, Backend, Request, Response};
use crate::errors::RvError;

#[cfg(not(feature = "sync_handler"))]
type SecretOperationHandler = dyn for<'a> Fn(
        &'a dyn Backend,
        &'a mut Request,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Response>, RvError>> + Send + 'a>>
    + Send
    + Sync;
#[cfg(feature = "sync_handler")]
type SecretOperationHandler = dyn Fn(&dyn Backend, &mut Request) -> Result<Option<Response>, RvError> + Send + Sync;

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

pub struct Secret {
    pub secret_type: String,
    pub default_duration: Duration,
    pub renew_handler: Option<Arc<SecretOperationHandler>>,
    pub revoke_handler: Option<Arc<SecretOperationHandler>>,
}

#[maybe_async::maybe_async]
impl Secret {
    pub fn renewable(&self) -> bool {
        self.renew_handler.is_some()
    }

    pub fn response(&self, data: Option<Map<String, Value>>, internal: Option<Map<String, Value>>) -> Response {
        let mut lease = Lease::default();
        lease.ttl = self.default_duration;
        lease.renewable = self.renewable();

        let mut secret = SecretData { lease, lease_id: String::new(), internal_data: Map::new() };

        if internal.is_some() {
            secret.internal_data.clone_from(internal.as_ref().unwrap());
        }

        secret.internal_data.insert("secret_type".to_owned(), Value::String(self.secret_type.clone()));

        let mut resp = Response::default();
        resp.data = data;
        resp.secret = Some(secret);
        resp
    }

    pub async fn renew(&self, backend: &dyn Backend, req: &mut Request) -> Result<Option<Response>, RvError> {
        if !self.renewable() || self.renew_handler.is_none() {
            return Err(RvError::ErrLogicalOperationUnsupported);
        }

        (self.renew_handler.as_ref().unwrap())(backend, req).await
    }

    pub async fn revoke(&self, backend: &dyn Backend, req: &mut Request) -> Result<Option<Response>, RvError> {
        if self.revoke_handler.is_none() {
            return Err(RvError::ErrLogicalOperationUnsupported);
        }

        (self.revoke_handler.as_ref().unwrap())(backend, req).await
    }
}

#[macro_export]
macro_rules! new_secret {
    ($($tt:tt)*) => {
        new_secret_internal!($($tt)*)
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! new_secret_internal {
    (@object $object:ident () {}) => {
    };
    (@object $object:ident () {secret_type: $secret_type:expr, $($rest:tt)*}) => {
        $object.secret_type = $secret_type.to_string();
        new_secret_internal!(@object $object () {$($rest)*});
    };
    (@object $object:ident () {default_duration: $duration:expr, $($rest:tt)*}) => {
        $object.default_duration = Duration::new($duration, 0);
        new_secret_internal!(@object $object () {$($rest)*});
    };
    (@object $object:ident () {renew_handler: $handler_obj:ident$(.$handler_method:ident)*, $($rest:tt)*}) => {
        $object.renew_handler = Some(Arc::new(move |backend, req| {
            let self_ = $handler_obj.clone();
            #[cfg(not(feature = "sync_handler"))]
            {
                Box::pin(async move {
                    self_$(.$handler_method)*(backend, req).await
                })
            }
            #[cfg(feature = "sync_handler")]
            self_$(.$handler_method)*(backend, req)
        }));
        new_secret_internal!(@object $object () {$($rest)*});
    };
    (@object $object:ident () {revoke_handler: $handler_obj:ident$(.$handler_method:ident)*, $($rest:tt)*}) => {
        $object.revoke_handler = Some(Arc::new(move |backend, req| {
            let self_ = $handler_obj.clone();
            #[cfg(not(feature = "sync_handler"))]
            {
                Box::pin(async move {
                    self_$(.$handler_method)*(backend, req).await
                })
            }
            #[cfg(feature = "sync_handler")]
            self_$(.$handler_method)*(backend, req)
        }));
        new_secret_internal!(@object $object () {$($rest)*});
    };
    ({ $($tt:tt)+ }) => {
        {
            let mut secret = Secret {
                secret_type: String::new(),
                default_duration: Duration::new(0, 0),
                renew_handler: None,
                revoke_handler: None,
            };
            new_secret_internal!(@object secret () {$($tt)+});
            secret
        }
    };
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

        pub async fn noop(&self, _backend: &dyn Backend, _req: &mut Request) -> Result<Option<Response>, RvError> {
            Ok(None)
        }
    }

    #[maybe_async::maybe_async]
    pub async fn noop(_backend: &dyn Backend, _req: &mut Request) -> Result<Option<Response>, RvError> {
        Ok(None)
    }

    #[test]
    fn test_logical_secret() {
        let t = Arc::new(MyTest::new());

        let secret: Secret = new_secret!({
            secret_type: "kv",
            default_duration: 60,
            renew_handler: t.noop,
            revoke_handler: noop,
        });

        assert_eq!(&secret.secret_type, "kv");
        assert_eq!(secret.default_duration, Duration::new(60, 0));
        assert!(secret.renew_handler.is_some());
        assert!(secret.revoke_handler.is_some());
    }
}
