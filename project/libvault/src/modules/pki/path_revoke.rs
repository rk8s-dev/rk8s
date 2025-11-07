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
            .pattern("revoke")
            .field(
                "serial_number",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description("Certificate serial number, in colon- or hyphen-separated octal"),
            )
            .operation(Operation::Write, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.revoke_cert(backend, req).await })
                }
            })
            .help(
                r#"
This allows certificates to be revoked using its serial number. A root token is required.
                "#,
            )
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
            .help(
                r#"
Force a rebuild of the CRL. This can be used to remove expired certificates from it if no certificates have been revoked. A root token is required.
                "#,
            )
            .build()
    }
}

impl PkiBackendInner {
    pub async fn revoke_cert(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let _payload: types::RevokeCertificateRequest = req.parse_json()?;
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
