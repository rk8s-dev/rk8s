//! The `cert` auth method allows authentication using SSL/TLS client certificates which are
//! either signed by a CA or self-signed. SSL/TLS client certificates are defined as having
//! an `ExtKeyUsage` extension with the usage set to either `ClientAuth` or `Any`.
//!
//! The trusted certificates and CAs are configured directly to the auth method using the
//! `certs/` path. This method cannot read trusted certificates from an external source.
//!
//! CA certificates are associated with a role; role names and CRL names are normalized
//! to lower-case.
//!
//! Please note that to use this auth method, `tls_disable` and `tls_disable_client_certs`
//! must be false in the RustyVault configuration. This is because the certificates are
//! sent through TLS communication itself.

use std::{any::Any, sync::Arc};

use async_trait::async_trait;
use dashmap::DashMap;
use derive_more::Deref;

use crate::{
    core::Core,
    errors::RvError,
    logical::{Backend, LogicalBackend},
    modules::{Module, auth::AuthModule},
};

pub mod path_certs;
pub mod path_config;
pub mod path_crls;
pub mod path_login;

pub use path_certs::CertEntry;
pub use path_crls::CRLInfo;

static CERT_BACKEND_HELP: &str = r#"
The "cert" credential provider allows authentication using
TLS client certificates. A client connects to RustyVault and uses
the "login" endpoint to generate a client token.

Trusted certificates are configured using the "certs/" endpoint
by a user with root access. A certificate authority can be trusted,
which permits all keys signed by it. Alternatively, self-signed
certificates can be trusted avoiding the need for a CA.
"#;

pub struct CertModule {
    pub name: String,
    pub backend: Arc<CertBackend>,
}

pub struct CertBackendInner {
    pub core: Arc<Core>,
    pub crls: DashMap<String, CRLInfo>,
}

#[derive(Deref)]
pub struct CertBackend {
    #[deref]
    pub inner: Arc<CertBackendInner>,
}

impl CertBackend {
    pub fn new(core: Arc<Core>) -> Self {
        let inner = CertBackendInner {
            core,
            crls: DashMap::new(),
        };
        Self {
            inner: Arc::new(inner),
        }
    }

    pub fn new_backend(&self) -> LogicalBackend {
        let builder = LogicalBackend::builder()
            .help(CERT_BACKEND_HELP)
            .unauth_paths(["login"])
            .path(self.config_path())
            .path(self.certs_path())
            .path(self.certs_list_path())
            .path(self.crl_path())
            .path(self.crl_list_path())
            .path(self.login_path());

        builder
            .auth_renew_handler({
                let handler = self.inner.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.login_renew(backend, req).await })
                }
            })
            .build()
    }
}

impl CertModule {
    pub fn new(core: Arc<Core>) -> Self {
        Self {
            name: "cert".to_string(),
            backend: Arc::new(CertBackend::new(core)),
        }
    }
}

#[async_trait]
impl Module for CertModule {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn as_any_arc(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }

    fn setup(&self, core: &Core) -> Result<(), RvError> {
        let cert = self.backend.clone();
        let cert_backend_new_func = move |_c: Arc<Core>| -> Result<Arc<dyn Backend>, RvError> {
            let mut cert_backend = cert.new_backend();
            cert_backend.init()?;
            Ok(Arc::new(cert_backend))
        };

        if let Some(auth_module) = core.module_manager.get_module::<AuthModule>("auth") {
            return auth_module.add_auth_backend("cert", Arc::new(cert_backend_new_func));
        } else {
            log::error!("get auth module failed!");
        }

        Ok(())
    }

    fn cleanup(&self, core: &Core) -> Result<(), RvError> {
        if let Some(auth_module) = core.module_manager.get_module::<AuthModule>("auth") {
            return auth_module.delete_auth_backend("cert");
        } else {
            log::error!("get auth module failed!");
        }

        Ok(())
    }
}
