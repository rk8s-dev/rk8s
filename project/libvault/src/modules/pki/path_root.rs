use serde_json::{Value, json};

use super::{PkiBackend, PkiBackendInner, field, util};
use crate::{
    errors::RvError,
    logical::{Backend, Operation, Path, Request, Response},
    utils,
};

impl PkiBackend {
    pub fn root_generate_path(&self) -> Path {
        let backend = self.inner.clone();

        let mut path = Path::builder()
            .pattern(r"root/generate/(?P<exported>.+)")
            .operation(Operation::Write, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.generate_root(backend, req).await })
                }
            })
            .help("Generate a new CA certificate and private key used for signing.")
            .build();

        path.fields.extend(field::ca_common_fields());
        path.fields.extend(field::ca_key_generation_fields());
        path.fields.extend(field::ca_issue_fields());

        path
    }

    pub fn root_delete_path(&self) -> Path {
        let backend = self.inner.clone();

        Path::builder()
            .pattern(r"root")
            .operation(Operation::Delete, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.delete_root(backend, req).await })
                }
            })
            .help("Deletes the root CA key to allow a new one to be generated.")
            .build()
    }
}

impl PkiBackendInner {
    pub async fn generate_root(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let mut export_private_key = false;
        if req
            .get_data_or_default("exported")?
            .as_str()
            .ok_or(RvError::ErrRequestFieldInvalid)?
            == "exported"
        {
            export_private_key = true;
        }

        let role_entry = util::get_role_params(req)?;

        let mut cert = util::generate_certificate(&role_entry, req)?;

        cert.is_ca = true;

        let cert_bundle = cert.to_cert_bundle(None, None)?;

        self.store_ca_bundle(req, &cert_bundle).await?;

        let cert_expiration =
            utils::asn1time_to_timestamp(cert_bundle.certificate.not_after().to_string().as_str())?;

        let mut resp_data = json!({
            "expiration": cert_expiration,
            "issuing_ca": String::from_utf8_lossy(&cert_bundle.certificate.to_pem()?),
            "certificate": String::from_utf8_lossy(&cert_bundle.certificate.to_pem()?),
            "serial_number": cert_bundle.serial_number.clone(),
        })
        .as_object()
        .unwrap()
        .clone();

        if export_private_key {
            resp_data.insert(
                "private_key".to_string(),
                Value::String(
                    String::from_utf8_lossy(&cert_bundle.private_key.private_key_to_pem_pkcs8()?)
                        .to_string(),
                ),
            );
            resp_data.insert(
                "private_key_type".to_string(),
                Value::String(cert_bundle.private_key_type.clone()),
            );
        }

        Ok(Some(Response::data_response(Some(resp_data))))
    }

    pub async fn delete_root(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        self.delete_ca_bundle(req).await?;
        Ok(None)
    }
}
