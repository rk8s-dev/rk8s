use crate::{
    errors::RvError,
    logical::{Backend, Operation, Path, Request, Response},
};

use super::{OpenPgpBackend, OpenPgpBackendInner, PGP_KEYS_PREFIX};

impl OpenPgpBackend {
    pub fn delete_path(&self) -> Path {
        let backend = self.inner.clone();
        Path::builder()
            .pattern(r"pgp/keys/(?P<name>\w[\w-]+\w)")
            .operation(Operation::Delete, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.delete_key(backend, req).await })
                }
            })
            .help("Delete an OpenPGP key")
            .build()
    }
}

impl OpenPgpBackendInner {
    pub async fn delete_key(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let name_val = req.get_data("name")?;
        let name = name_val
            .as_str()
            .ok_or(RvError::ErrRequestFieldInvalid)?
            .to_string();

        let user = req
            .auth
            .as_ref()
            .map(|a| a.display_name.as_str())
            .unwrap_or("unknown");
        log::info!("User [{}] is deleting PGP key [{}]", user, name);

        let entry = req
            .storage_get(&format!("{}{}", PGP_KEYS_PREFIX, name))
            .await?;
        if entry.is_none() {
            return Ok(None);
        }

        req.storage_delete(&format!("{}{}", PGP_KEYS_PREFIX, name))
            .await?;
        Ok(None)
    }
}
