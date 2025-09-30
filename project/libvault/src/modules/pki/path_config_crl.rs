use std::{collections::HashMap, sync::Arc};

use super::{PkiBackend, PkiBackendInner};
use crate::{
    context::Context,
    errors::RvError,
    logical::{Backend, Field, FieldType, Operation, Path, PathOperation, Request, Response},
    new_fields, new_fields_internal, new_path, new_path_internal,
};

impl PkiBackend {
    pub fn config_crl_path(&self) -> Path {
        let pki_backend_ref1 = self.inner.clone();
        let pki_backend_ref2 = self.inner.clone();

        let path = new_path!({
            pattern: "config/crl",
            fields: {
                "expiry": {
                    field_type: FieldType::Str,
                    default: "72h",
                    description: "The amount of time the generated CRL should be valid; defaults to 72 hours"
                }
            },
            operations: [
                {op: Operation::Read, handler: pki_backend_ref1.read_path_crl},
                {op: Operation::Write, handler: pki_backend_ref2.write_path_crl}
            ],
            help: r#"
This endpoint allows configuration of the CRL lifetime.
                "#
        });

        path
    }
}

#[maybe_async::maybe_async]
impl PkiBackendInner {
    pub async fn read_path_crl(&self, _backend: &dyn Backend, _req: &mut Request) -> Result<Option<Response>, RvError> {
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
