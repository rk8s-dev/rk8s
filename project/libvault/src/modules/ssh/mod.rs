use std::{any::Any, fmt, sync::Arc};

use async_trait::async_trait;
use derive_more::Deref;

use crate::{
    core::Core,
    errors::RvError,
    logical::{Backend, LogicalBackend},
    modules::Module,
};

pub mod path_config_ca;
pub mod path_fetch;
pub mod path_inspect;
pub mod path_issue;
pub mod path_keys;
pub mod path_revoke;
pub mod path_roles;
pub mod path_verify;
pub mod types;

static SSH_BACKEND_HELP: &str = r#"
The SSH backend provides an SSH CA (Certificate Authority) to manage and sign SSH keys.
It supports:
- Generating and managing SSH CA keys
- Signing SSH public keys (User and Host certificates)
- Managing roles and policies for signing
- Revoking keys and certificates
- Storing and managing SSH user keys
"#;

pub(crate) const SSH_CA_PREFIX: &str = "ssh/ca/";
pub(crate) const SSH_KEYS_PREFIX: &str = "ssh/keys/";
pub(crate) const SSH_CERTS_PREFIX: &str = "ssh/certs/";
pub(crate) const SSH_ROLES_PREFIX: &str = "ssh/roles/";
pub(crate) const SSH_REVOKED_PREFIX: &str = "ssh/revoked/";
pub(crate) const SSH_SERIAL_PATH: &str = "ssh/config/serial";

#[derive(Debug, Clone)]
pub struct SshModule {
    pub name: String,
    pub backend: Arc<SshBackend>,
}

pub struct SshBackendInner {
    pub core: Arc<Core>,
}

impl fmt::Debug for SshBackendInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SshBackendInner").finish()
    }
}

#[derive(Clone, Deref)]
pub struct SshBackend {
    #[deref]
    pub inner: Arc<SshBackendInner>,
}

impl fmt::Debug for SshBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SshBackend").finish()
    }
}

impl SshBackend {
    pub fn new(core: Arc<Core>) -> Self {
        Self {
            inner: Arc::new(SshBackendInner { core }),
        }
    }

    pub fn new_backend(&self) -> LogicalBackend {
        let builder = LogicalBackend::builder()
            .help(SSH_BACKEND_HELP)
            .root_paths(["ssh/*"])
            .path(self.ca_generate_path())
            .path(self.key_generate_path())
            .path(self.keys_list_path())
            .path(self.cert_sign_path())
            .path(self.cert_fetch_path())
            .path(self.certs_list_path())
            .path(self.role_path())
            .path(self.roles_list_path())
            .path(self.revoke_path())
            .path(self.revoked_list_path())
            .path(self.config_ca_path())
            .path(self.public_ca_path())
            .path(self.verify_path())
            .path(self.inspect_path());

        builder.build()
    }
}

impl SshModule {
    pub fn new(core: Arc<Core>) -> Self {
        Self {
            name: "ssh".to_string(),
            backend: Arc::new(SshBackend::new(core)),
        }
    }
}

#[async_trait]
impl Module for SshModule {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn as_any_arc(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }

    fn setup(&self, core: &Core) -> Result<(), RvError> {
        let ssh = self.backend.clone();
        let new_func = move |_c: Arc<Core>| -> Result<Arc<dyn Backend>, RvError> {
            let mut b = ssh.new_backend();
            b.init()?;
            Ok(Arc::new(b))
        };
        core.add_logical_backend("ssh", Arc::new(new_func))
    }
}
