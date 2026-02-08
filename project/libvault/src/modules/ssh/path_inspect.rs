use anyhow::anyhow;
use serde_json::json;
use ssh_key::Certificate;

use crate::{
    errors::RvError,
    logical::{Backend, Field, FieldType, Operation, Path, Request, Response},
};

use super::{SshBackend, SshBackendInner};

impl SshBackend {
    pub fn inspect_path(&self) -> Path {
        let backend = self.inner.clone();
        Path::builder()
            .pattern("inspect")
            .operation(Operation::Write, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.inspect_cert(backend, req).await })
                }
            })
            .field(
                "certificate",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("SSH Certificate"),
            )
            .help("Inspect an SSH certificate")
            .build()
    }
}

impl SshBackendInner {
    pub async fn inspect_cert(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let cert_str = req
            .get_data("certificate")?
            .as_str()
            .ok_or(RvError::ErrRequestFieldInvalid)?
            .to_string();
        let cert = Certificate::from_openssh(&cert_str)
            .map_err(|e| RvError::ErrOther(anyhow!("Failed to parse certificate: {}", e)))?;

        let valid_after = cert.valid_after();
        let valid_before = cert.valid_before();
        let key_id = cert.key_id();
        let serial = cert.serial();
        let principals = cert.valid_principals();
        let extensions: std::collections::BTreeMap<String, String> = cert
            .extensions()
            .iter()
            .map(|(k, v): (&String, &String)| (k.clone(), v.clone()))
            .collect();
        let critical_options: std::collections::BTreeMap<String, String> = cert
            .critical_options()
            .iter()
            .map(|(k, v): (&String, &String)| (k.clone(), v.clone()))
            .collect();
        let cert_type = format!("{:?}", cert.cert_type());
        let key_algo = cert.public_key().algorithm().to_string();

        let resp = json!({
            "key_id": key_id,
            "serial": serial,
            "valid_after": valid_after,
            "valid_before": valid_before,
            "principals": principals,
            "cert_type": cert_type,
            "key_algo": key_algo,
            "extensions": extensions,
            "critical_options": critical_options,
        });

        Ok(Some(Response::data_response(Some(
            resp.as_object()
                .ok_or(RvError::ErrResponseDataInvalid)?
                .clone(),
        ))))
    }
}
