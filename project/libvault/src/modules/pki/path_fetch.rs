use openssl::x509::X509;

use super::{
    CertBackend, PgpCertBackend, PkiBackend, PkiBackendInner, SshCertBackend, TlsCertBackend, types,
};
use crate::{
    errors::RvError,
    logical::{Backend, Field, FieldType, Operation, Path, Request, Response},
    modules::ResponseExt,
    utils::cert::CertBundle,
};

impl PkiBackend {
    /// `ca/(?P<cert_type>tls|ssh)(/pem)?`
    pub fn fetch_ca_path(&self) -> Path {
        let backend = self.inner.clone();

        Path::builder()
            .pattern(r"ca/(?P<cert_type>tls|ssh)(/pem)?")
            .field(
                "cert_type",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("Certificate type: tls or ssh"),
            )
            .operation(Operation::Read, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.dispatch_fetch_ca(backend, req).await })
                }
            })
            .help("Fetch the CA certificate (TLS) or public key (SSH).")
            .build()
    }

    /// `crl(/pem)?` — TLS only
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
            .help("Fetch the CRL.")
            .build()
    }

    /// `cert/(?P<cert_type>tls|ssh)/(?P<serial>[0-9A-Fa-f-:]+)`
    pub fn fetch_cert_path(&self) -> Path {
        let backend = self.inner.clone();

        Path::builder()
            .pattern(r"cert/(?P<cert_type>tls|ssh)/(?P<serial>[0-9A-Fa-f-:]+)")
            .field(
                "cert_type",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("Certificate type: tls or ssh"),
            )
            .field(
                "serial",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description("Certificate serial number"),
            )
            .operation(Operation::Read, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.dispatch_fetch_cert(backend, req).await })
                }
            })
            .help("Fetch a certificate by serial number.")
            .build()
    }

    /// `cert/crl` — TLS CRL cert
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
            .help("Fetch the CRL certificate.")
            .build()
    }

    /// `certs/(?P<cert_type>tls|ssh|pgp)/?$`
    pub fn list_certs_path(&self) -> Path {
        let backend = self.inner.clone();

        Path::builder()
            .pattern(r"certs/(?P<cert_type>tls|ssh|pgp)/?$")
            .field(
                "cert_type",
                Field::builder()
                    .field_type(FieldType::Str)
                    .required(true)
                    .description("Certificate type: tls, ssh, or pgp"),
            )
            .operation(Operation::List, {
                let handler = backend.clone();
                move |backend, req| {
                    let handler = handler.clone();
                    Box::pin(async move { handler.dispatch_list_certs(backend, req).await })
                }
            })
            .help("List certificates.")
            .build()
    }
}

impl PkiBackendInner {
    // ── Dispatch ──

    pub async fn dispatch_fetch_ca(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let ct = req.get_data("cert_type")?;
        let ct = ct.as_str().ok_or(RvError::ErrRequestFieldInvalid)?;
        match ct {
            "tls" => self.read_path_fetch_ca(_backend, req).await,
            "ssh" => self.read_path_fetch_ssh_ca(_backend, req).await,
            _ => Err(RvError::ErrRequestFieldInvalid),
        }
    }

    pub async fn dispatch_fetch_cert(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let ct = req.get_data("cert_type")?;
        let ct = ct.as_str().ok_or(RvError::ErrRequestFieldInvalid)?;
        match ct {
            "tls" => self.read_path_fetch_cert(_backend, req).await,
            "ssh" => self.read_path_fetch_ssh_cert(_backend, req).await,
            _ => Err(RvError::ErrRequestFieldInvalid),
        }
    }

    pub async fn dispatch_list_certs(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let ct = req.get_data("cert_type")?;
        let ct = ct.as_str().ok_or(RvError::ErrRequestFieldInvalid)?;
        match ct {
            "tls" => self.list_certs(_backend, req).await,
            "ssh" => self.list_ssh_certs(_backend, req).await,
            "pgp" => self.list_pgp_keys(_backend, req).await,
            _ => Err(RvError::ErrRequestFieldInvalid),
        }
    }

    // ── TLS CA fetch ──

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

    // ── SSH CA fetch ──

    pub async fn read_path_fetch_ssh_ca(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let bundle = self.fetch_ssh_ca_bundle(req).await?;
        let response = types::SshConfigCaResponse {
            public_key: bundle.public_key_openssh,
        };
        Ok(Some(Response::data_response(response.to_map()?)))
    }

    // ── CRL ──

    pub async fn read_path_fetch_crl(
        &self,
        _backend: &dyn Backend,
        _req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        Ok(None)
    }

    // ── TLS cert fetch ──

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

    // ── SSH cert fetch ──

    pub async fn read_path_fetch_ssh_cert(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let serial_value = req.get_data("serial")?;
        let serial = serial_value
            .as_str()
            .ok_or(RvError::ErrRequestFieldInvalid)?;

        let signed_key = self.fetch_ssh_cert(req, serial).await?;

        let response = types::SshSignKeyResponse {
            signed_key,
            serial_number: serial.to_string(),
            expiration: 0,
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

    // ── List ──

    pub async fn list_certs(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let keys = req.storage_list("certs/tls/").await?;
        Ok(Some(Response::list_response(&keys)))
    }

    pub async fn list_ssh_certs(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let keys = req.storage_list("certs/ssh/").await?;
        Ok(Some(Response::list_response(&keys)))
    }

    pub async fn list_pgp_keys(
        &self,
        _backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        let keys = req.storage_list("certs/pgp/").await?;
        Ok(Some(Response::list_response(&keys)))
    }

    // ── Storage helpers ──

    pub async fn fetch_cert(&self, req: &Request, serial_number: &str) -> Result<X509, RvError> {
        TlsCertBackend.fetch_cert(req, serial_number).await
    }

    pub async fn store_cert(
        &self,
        req: &Request,
        serial_number: &str,
        cert: &X509,
    ) -> Result<(), RvError> {
        TlsCertBackend.store_cert(req, serial_number, cert).await
    }

    pub async fn delete_cert(&self, req: &Request, serial_number: &str) -> Result<(), RvError> {
        TlsCertBackend.delete_cert(req, serial_number).await
    }

    pub async fn store_ssh_cert(
        &self,
        req: &Request,
        serial_hex: &str,
        signed_key: &String,
    ) -> Result<(), RvError> {
        SshCertBackend.store_cert(req, serial_hex, signed_key).await
    }

    pub async fn fetch_ssh_cert(&self, req: &Request, serial_hex: &str) -> Result<String, RvError> {
        SshCertBackend.fetch_cert(req, serial_hex).await
    }

    pub async fn fetch_pgp_key(
        &self,
        req: &Request,
        key_name: &str,
    ) -> Result<types::PgpKeyBundle, RvError> {
        PgpCertBackend.fetch_cert(req, key_name).await
    }
}
