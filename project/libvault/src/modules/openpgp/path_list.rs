use crate::{
    errors::RvError,
    logical::{Backend, Operation, Path, Request, Response},
};

use super::{OpenPgpBackend, PGP_KEYS_PREFIX};

impl OpenPgpBackend {
    pub fn list_path(&self) -> Path {
        let backend = self.inner.clone();
        Path::builder()
            .pattern("pgp/keys/?")
            .operation(Operation::List, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.list_keys(backend, req).await })
                }
            })
            .help("List generated OpenPGP keys")
            .build()
    }
}

// Implement list_keys on OpenPgpBackendInner but we need to define it first in mod or import it?
// Actually OpenPgpBackendInner methods are usually defined in impl blocks in separate files.
// But Rust allows multiple impl blocks.

use super::OpenPgpBackendInner;

impl OpenPgpBackendInner {
    pub async fn list_keys(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let keys = req.storage_list(PGP_KEYS_PREFIX).await?;
        Ok(Some(Response::list_response(&keys)))
    }
}
