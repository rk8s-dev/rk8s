use super::{PkiBackend, PkiBackendInner, types};
use crate::{
    errors::RvError,
    logical::{Backend, Field, FieldType, Operation, Path, Request, Response},
    modules::RequestExt,
};

impl PkiBackend {
    pub fn revoke_path(&self) -> Path {
        let backend = self.inner.clone();

        Path::builder()
            .pattern(r"revoke/(?P<cert_type>tls|ssh|pgp)")
            .field(
                "cert_type",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("Certificate type: tls, ssh, or pgp"),
            )
            .field(
                "serial_number",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description("Certificate serial number (TLS/SSH)"),
            )
            .field(
                "key_name",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description("PGP key name (PGP)"),
            )
            .operation(Operation::Write, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.dispatch_revoke(backend, req).await })
                }
            })
            .help("Revoke a certificate or PGP key.")
            .build()
    }

    pub fn crl_rotate_path(&self) -> Path {
        let backend = self.inner.clone();

        Path::builder()
            .pattern("crl/rotate")
            .operation(Operation::Read, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.read_rotate_crl(backend, req).await })
                }
            })
            .help("Force a rebuild of the CRL.")
            .build()
    }
}

impl PkiBackendInner {
    pub async fn dispatch_revoke(
        &self,
        backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let ct = req.get_data("cert_type")?;
        let ct = ct.as_str().ok_or(RvError::ErrRequestFieldInvalid)?;
        match ct {
            "tls" => self.revoke_cert(backend, req).await,
            "ssh" => self.revoke_ssh_cert(backend, req).await,
            "pgp" => self.revoke_pgp_key(backend, req).await,
            _ => Err(RvError::ErrRequestFieldInvalid),
        }
    }

    pub async fn revoke_cert(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let _payload: types::RevokeCertificateRequest = req.parse_json()?;
        Ok(None)
    }

    pub async fn revoke_ssh_cert(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let _payload: types::RevokeCertificateRequest = req.parse_json()?;
        // TODO: record revocation for KRL
        Ok(None)
    }

    pub async fn revoke_pgp_key(
        &self,
        _backend: &dyn Backend,
        _req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        // TODO: generate PGP revocation signature
        Ok(None)
    }

    pub async fn read_rotate_crl(
        &self,
        _backend: &dyn Backend,
        _req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        Ok(None)
    }
}
