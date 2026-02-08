use serde_json::json;
use ssh_key::Certificate;

use crate::{
    errors::RvError,
    logical::{Backend, Field, FieldType, Operation, Path, Request, Response},
};

use super::{SSH_CERTS_PREFIX, SshBackend, SshBackendInner};

impl SshBackend {
    pub fn cert_fetch_path(&self) -> Path {
        let backend = self.inner.clone();
        Path::builder()
            .pattern(r"cert/fetch/(?P<name>\w[\w-]+\w)")
            .operation(Operation::Read, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.fetch_cert(backend, req).await })
                }
            })
            .field(
                "name",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("Certificate name"),
            )
            .help("Fetch a stored SSH certificate")
            .build()
    }

    pub fn certs_list_path(&self) -> Path {
        let backend = self.inner.clone();
        Path::builder()
            .pattern("certs/?")
            .operation(Operation::List, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.list_certs(backend, req).await })
                }
            })
            .help("List signed SSH certificates")
            .build()
    }
}

impl SshBackendInner {
    pub async fn fetch_cert(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let name_val = req.get_data("name")?;
        let name = name_val.as_str().ok_or(RvError::ErrRequestFieldInvalid)?;

        // Permission check: ensure we have an authenticated user
        if req.client_token.is_empty() {
            return Err(RvError::ErrRequestClientTokenMissing);
        }

        let user = req
            .auth
            .as_ref()
            .map(|a| a.display_name.as_str())
            .unwrap_or("unknown");
        log::info!("User [{}] is fetching SSH certificate [{}]", user, name);

        let entry = req
            .storage_get(format!("{SSH_CERTS_PREFIX}{name}").as_str())
            .await?
            .ok_or(RvError::ErrPkiCertNotFound)?;

        // Strict parsing + Fault tolerance (Log error but fail safe)
        let cert_str = String::from_utf8(entry.value)?;

        if let Err(e) = Certificate::from_openssh(&cert_str) {
            log::error!(
                "Stored data for '{}' is not a valid SSH certificate: {}",
                name,
                e
            );
            return Err(RvError::ErrPkiDataInvalid); // Or ErrSshKeyError if accessible
        }

        let resp = json!({ "certificate": cert_str })
            .as_object()
            .ok_or(RvError::ErrResponse(
                "Failed to construct response object".into(),
            ))?
            .clone();

        Ok(Some(Response::data_response(Some(resp))))
    }

    pub async fn list_certs(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let certs = req.storage_list(SSH_CERTS_PREFIX).await?;
        Ok(Some(Response::list_response(&certs)))
    }
}
