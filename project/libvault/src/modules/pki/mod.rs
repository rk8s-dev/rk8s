//! The `libvault::pki` module implements public key cryptography features, including
//! manipulating certificates as a CA or encrypting a piece of data by using a public key.

use crate::{
    core::Core,
    errors::RvError,
    logical::{Backend, LogicalBackend, Request, Response, SecretBuilder},
    modules::Module,
};
use anyhow::Context;
use async_trait::async_trait;
use derive_more::Deref;
use rustls::pki_types::CertificateDer;
use std::io::Cursor;
use std::time::SystemTime;
use std::{
    any::Any,
    convert::TryFrom,
    sync::{Arc, atomic::AtomicU64},
    time::Duration,
};
use x509_parser::nom::AsBytes;
use x509_parser::prelude::{FromDer, X509Certificate};
use x509_parser::time::ASN1Time;

pub mod field;
pub mod path_config_ca;
pub mod path_config_crl;
pub mod path_fetch;
pub mod path_issue;
pub mod path_keys;
pub mod path_revoke;
pub mod path_roles;
pub mod path_root;
pub mod types;
pub mod util;

static PKI_BACKEND_HELP: &str = r#"
The PKI backend dynamically generates X509 server and client certificates.

After mounting this backend, configure the CA using the "pem_bundle" endpoint within
the "config/" path.
"#;
const _DEFAULT_LEASE_TTL: Duration = Duration::from_secs(3600_u64);

pub struct PkiModule {
    pub name: String,
    pub backend: Arc<PkiBackend>,
}

pub struct PkiBackendInner {
    pub core: Arc<Core>,
    pub cert_count: AtomicU64,
    pub revoked_cert_count: AtomicU64,
}

#[derive(Deref)]
pub struct PkiBackend {
    #[deref]
    pub inner: Arc<PkiBackendInner>,
}

impl PkiBackend {
    pub fn new(core: Arc<Core>) -> Self {
        Self {
            inner: Arc::new(PkiBackendInner {
                core,
                cert_count: AtomicU64::new(0),
                revoked_cert_count: AtomicU64::new(0),
            }),
        }
    }

    pub fn new_backend(&self) -> LogicalBackend {
        let builder = LogicalBackend::builder()
            .help(PKI_BACKEND_HELP)
            .root_paths(["config/*", "revoke/*", "crl/rotate"])
            .unauth_paths(["cert/*", "ca/pem", "ca", "crl", "crl/pem"])
            .path(self.roles_path())
            .path(self.config_ca_path())
            .path(self.root_generate_path())
            .path(self.root_delete_path())
            .path(self.fetch_ca_path())
            .path(self.fetch_crl_path())
            .path(self.fetch_cert_path())
            .path(self.fetch_cert_crl_path())
            .path(self.issue_path())
            .path(self.revoke_path())
            .path(self.crl_rotate_path())
            .path(self.keys_generate_path())
            .path(self.keys_import_path())
            .path(self.keys_sign_path())
            .path(self.keys_verify_path())
            .path(self.keys_encrypt_path())
            .path(self.keys_decrypt_path());

        let secret = SecretBuilder::new()
            .secret_type("pki")
            .revoke_handler({
                let handler = self.inner.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.revoke_secret_creds(backend, req).await })
                }
            })
            .renew_handler({
                let handler = self.inner.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.renew_secret_creds(backend, req).await })
                }
            })
            .build();

        builder.secret(secret).build()
    }
}

impl PkiBackendInner {
    pub async fn revoke_secret_creds(
        &self,
        _backend: &dyn Backend,
        _req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        Ok(None)
    }
    pub async fn renew_secret_creds(
        &self,
        _backend: &dyn Backend,
        _req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        Ok(None)
    }
}

impl PkiModule {
    pub fn new(core: Arc<Core>) -> Self {
        Self {
            name: "pki".to_string(),
            backend: Arc::new(PkiBackend::new(core)),
        }
    }
}

#[async_trait]
impl Module for PkiModule {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn as_any_arc(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> {
        self
    }

    fn setup(&self, core: &Core) -> Result<(), RvError> {
        let pki = self.backend.clone();
        let pki_backend_new_func = move |_c: Arc<Core>| -> Result<Arc<dyn Backend>, RvError> {
            let mut pki_backend = pki.new_backend();
            pki_backend.init()?;
            Ok(Arc::new(pki_backend))
        };
        core.add_logical_backend("pki", Arc::new(pki_backend_new_func))
    }

    fn cleanup(&self, core: &Core) -> Result<(), RvError> {
        core.delete_logical_backend("pki")
    }
}

pub trait CertExt {
    fn to_certs(&self) -> anyhow::Result<Vec<CertificateDer<'static>>> {
        Ok(vec![])
    }

    fn validity_range(&self) -> anyhow::Result<(ASN1Time, ASN1Time)> {
        anyhow::bail!("Not implemented")
    }

    fn rotate_deadline(&self, ratio: f64) -> anyhow::Result<SystemTime> {
        anyhow::ensure!(ratio.is_finite(), "Rotation ratio must be finite");
        anyhow::ensure!(
            (0.0..=1.0).contains(&ratio),
            "Rotation ratio must lie in the inclusive range [0.0, 1.0]",
        );

        let (not_before, not_after) = self.validity_range()?;
        let not_before = not_before.to_datetime();
        let not_after = not_after.to_datetime();

        let lifetime = not_after - not_before;
        anyhow::ensure!(
            !lifetime.is_negative(),
            "Certificate validity window is inverted",
        );

        let rotate_at = not_before + lifetime * ratio;
        let nanos = rotate_at.unix_timestamp_nanos();

        if nanos >= 0 {
            let secs = u64::try_from(nanos / 1_000_000_000)
                .context("Rotation deadline is too far in the future")?;
            let sub_nanos = u32::try_from(nanos % 1_000_000_000)
                .context("Nanosecond remainder out of range")?;
            return Ok(SystemTime::UNIX_EPOCH + Duration::new(secs, sub_nanos));
        }

        let nanos_abs = nanos
            .checked_abs()
            .ok_or_else(|| anyhow::anyhow!("Rotation deadline overflow"))?;
        let secs = u64::try_from(nanos_abs / 1_000_000_000)
            .context("Rotation deadline is too far in the past")?;
        let sub_nanos = u32::try_from(nanos_abs % 1_000_000_000)
            .context("Nanosecond remainder out of range")?;
        let duration = Duration::new(secs, sub_nanos);
        SystemTime::UNIX_EPOCH
            .checked_sub(duration)
            .context("Rotation deadline before supported SystemTime range")
    }
}

impl CertExt for String {
    fn to_certs(&self) -> anyhow::Result<Vec<CertificateDer<'static>>> {
        let mut reader = Cursor::new(self.as_bytes());
        rustls_pemfile::certs(&mut reader)
            .map(|e| e.with_context(|| "Failed to extract certificate from PEM certificate chain"))
            .collect::<anyhow::Result<Vec<_>>>()
    }

    fn validity_range(&self) -> anyhow::Result<(ASN1Time, ASN1Time)> {
        let certs = self.to_certs()?;
        anyhow::ensure!(!certs.is_empty(), "No valid certs found");
        certs[0].validity_range()
    }
}

impl CertExt for CertificateDer<'static> {
    fn to_certs(&self) -> anyhow::Result<Vec<CertificateDer<'static>>> {
        Ok(vec![self.clone()])
    }

    fn validity_range(&self) -> anyhow::Result<(ASN1Time, ASN1Time)> {
        let (_, parsed) = X509Certificate::from_der(self.as_bytes())?;
        let validity = parsed.validity();
        Ok((validity.not_before, validity.not_after))
    }
}
