use openssl::x509::X509;

use super::{PkiBackend, PkiBackendInner, types};
use crate::{
    errors::RvError,
    logical::{Backend, Field, FieldType, Operation, Path, Request, Response},
    modules::ResponseExt,
    storage::StorageEntry,
    utils::cert::CertBundle,
};

impl PkiBackend {
    pub fn fetch_ca_path(&self) -> Path {
        let backend = self.inner.clone();

        Path::builder()
            .pattern("ca(/pem)?")
            .operation(Operation::Read, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.read_path_fetch_ca(backend, req).await })
                }
            })
            .help(
                r#"
This allows certificates to be fetched. If using the fetch/ prefix any non-revoked certificate can be fetched.
Using "ca" or "crl" as the value fetches the appropriate information in DER encoding. Add "/pem" to either to get PEM encoding.
                "#,
            )
            .build()
    }

    pub fn fetch_crl_path(&self) -> Path {
        let backend = self.inner.clone();

        Path::builder()
            .pattern("crl(/pem)?")
            .operation(Operation::Read, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.read_path_fetch_crl(backend, req).await })
                }
            })
            .help(
                r#"
This allows certificates to be fetched. If using the fetch/ prefix any non-revoked certificate can be fetched.
Using "ca" or "crl" as the value fetches the appropriate information in DER encoding. Add "/pem" to either to get PEM encoding.
                "#,
            )
            .build()
    }

    pub fn fetch_cert_path(&self) -> Path {
        let backend = self.inner.clone();

        Path::builder()
            .pattern(r"cert/(?P<serial>[0-9A-Fa-f-:]+)")
            .field(
                "serial",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description(
                        "Certificate serial number, in colon- or hyphen-separated octal",
                    ),
            )
            .operation(Operation::Read, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.read_path_fetch_cert(backend, req).await })
                }
            })
            .help(
                r#"
This allows certificates to be fetched. If using the fetch/ prefix any non-revoked certificate can be fetched.
Using "ca" or "crl" as the value fetches the appropriate information in DER encoding. Add "/pem" to either to get PEM encoding.
                "#,
            )
            .build()
    }

    pub fn fetch_cert_crl_path(&self) -> Path {
        let backend = self.inner.clone();

        Path::builder()
            .pattern("cert/crl")
            .operation(Operation::Read, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.read_path_fetch_cert_crl(backend, req).await })
                }
            })
            .help(
                r#"
This allows certificates to be fetched. If using the fetch/ prefix any non-revoked certificate can be fetched.
Using "ca" or "crl" as the value fetches the appropriate information in DER encoding. Add "/pem" to either to get PEM encoding.
                "#,
            )
            .build()
    }
}

impl PkiBackendInner {
    pub async fn handle_fetch_cert_bundle(
        &self,
        cert_bundle: &CertBundle,
    ) -> Result<Option<Response>, RvError> {
        let ca_chain_pem: String = cert_bundle
            .ca_chain
            .iter()
            .rev()
            .map(|x509| x509.to_pem().unwrap())
            .map(|pem| String::from_utf8_lossy(&pem).to_string())
            .collect::<Vec<String>>()
            .join("");

        let response = types::FetchCaResponse {
            certificate: String::from_utf8_lossy(&cert_bundle.certificate.to_pem()?).to_string(),
            ca_chain: Some(ca_chain_pem),
            issuing_ca: None,
            serial_number: Some(cert_bundle.serial_number.clone()),
        };

        Ok(Some(Response::data_response(response.to_map()?)))
    }

    pub async fn read_path_fetch_ca(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let ca_bundle = self.fetch_ca_bundle(req).await?;
        self.handle_fetch_cert_bundle(&ca_bundle).await
    }

    pub async fn read_path_fetch_crl(
        &self,
        _backend: &dyn Backend,
        _req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        Ok(None)
    }

    pub async fn read_path_fetch_cert(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let serial_number_value = req.get_data("serial")?;
        let serial_number = serial_number_value
            .as_str()
            .ok_or(RvError::ErrRequestFieldInvalid)?;
        let serial_number_hex = serial_number.replace(':', "-").to_lowercase();
        let cert = self.fetch_cert(req, &serial_number_hex).await?;
        let ca_bundle = self.fetch_ca_bundle(req).await?;

        let mut ca_chain_pem: String = ca_bundle
            .ca_chain
            .iter()
            .rev()
            .map(|x509| x509.to_pem().unwrap())
            .map(|pem| String::from_utf8_lossy(&pem).to_string())
            .collect::<Vec<String>>()
            .join("");

        ca_chain_pem =
            ca_chain_pem + &String::from_utf8_lossy(&ca_bundle.certificate.to_pem().unwrap());

        let response = types::FetchCertificateResponse {
            ca_chain: ca_chain_pem,
            certificate: String::from_utf8_lossy(&cert.to_pem()?).to_string(),
            serial_number: serial_number.to_string(),
        };

        Ok(Some(Response::data_response(response.to_map()?)))
    }

    pub async fn read_path_fetch_cert_crl(
        &self,
        _backend: &dyn Backend,
        _req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        Ok(None)
    }

    pub async fn fetch_cert(&self, req: &Request, serial_number: &str) -> Result<X509, RvError> {
        let entry = req
            .storage_get(format!("certs/{serial_number}").as_str())
            .await?;
        if entry.is_none() {
            return Err(RvError::ErrPkiCertNotFound);
        }

        let cert: X509 = X509::from_der(entry.unwrap().value.as_slice())?;
        Ok(cert)
    }

    pub async fn store_cert(
        &self,
        req: &Request,
        serial_number: &str,
        cert: &X509,
    ) -> Result<(), RvError> {
        let value = cert.to_der()?;
        let entry = StorageEntry {
            key: format!("certs/{serial_number}"),
            value,
        };
        req.storage_put(&entry).await?;
        Ok(())
    }

    pub async fn delete_cert(&self, req: &Request, serial_number: &str) -> Result<(), RvError> {
        req.storage_delete(format!("certs/{serial_number}").as_str())
            .await?;
        Ok(())
    }
}
