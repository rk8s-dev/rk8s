use super::{PkiBackend, PkiBackendInner};
use crate::{
    errors::RvError,
    logical::{Backend, Field, FieldType, Operation, Path, Request, Response},
};

impl PkiBackend {
    pub fn config_crl_path(&self) -> Path {
        let backend_read = self.inner.clone();
        let backend_write = self.inner.clone();

        Path::builder()
            .pattern("config/crl")
            .field(
                "expiry",
                Field::builder()
                    .field_type(FieldType::Str)
                    .default_value("72h")
                    .description(
                        "The amount of time the generated CRL should be valid; defaults to 72 hours",
                    ),
            )
            .operation(Operation::Read, {
                let handler = backend_read.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.read_path_crl(backend, req).await })
                }
            })
            .operation(Operation::Write, {
                let handler = backend_write.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.write_path_crl(backend, req).await })
                }
            })
            .help("This endpoint allows configuration of the CRL lifetime.")
            .build()
    }
}

impl PkiBackendInner {
    pub async fn read_path_crl(
        &self,
        _backend: &dyn Backend,
        _req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        Ok(None)
    }

    pub async fn write_path_crl(
        &self,
        _backend: &dyn Backend,
        _req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        Ok(None)
    }
}
