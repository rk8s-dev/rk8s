use std::{fs, io::BufReader, path::PathBuf, sync::Arc};

use better_default::Default;
use rustls::{
    ALL_VERSIONS, ClientConfig, RootCertStore,
    pki_types::{PrivateKeyDer, pem::PemObject},
};
use webpki_roots::TLS_SERVER_ROOTS;

use crate::{errors::RvError, utils::cert::DisabledVerifier};

#[derive(Clone)]
pub struct TLSConfig {
    client_config: ClientConfig,
    server_ca_pem: Option<Vec<u8>>,
    client_cert_pem: Option<Vec<u8>>,
    client_key_pem: Option<Vec<u8>>,
    insecure: bool,
}

#[derive(Default, Clone)]
pub struct TLSConfigBuilder {
    pub server_ca_pem: Option<Vec<u8>>,
    pub client_cert_pem: Option<Vec<u8>>,
    pub client_key_pem: Option<Vec<u8>>,
    pub tls_server_name: Option<String>,
    pub insecure: bool,
}

impl TLSConfigBuilder {
    pub fn new() -> Self {
        TLSConfigBuilder::default()
    }

    pub fn with_server_ca_path(mut self, server_ca_path: &PathBuf) -> Result<Self, RvError> {
        let cert_data = fs::read(server_ca_path)?;
        self.server_ca_pem = Some(cert_data);
        Ok(self)
    }

    pub fn with_server_ca_pem(mut self, server_ca_pem: &str) -> Self {
        self.server_ca_pem = Some(server_ca_pem.as_bytes().to_vec());
        self
    }

    pub fn with_client_cert_path(
        mut self,
        client_cert_path: &PathBuf,
        client_key_path: &PathBuf,
    ) -> Result<Self, RvError> {
        let cert_data = fs::read(client_cert_path)?;
        self.client_cert_pem = Some(cert_data);

        let key_data = fs::read(client_key_path)?;
        self.client_key_pem = Some(key_data);

        Ok(self)
    }

    pub fn with_client_cert_pem(mut self, client_cert_pem: &str, client_key_pem: &str) -> Self {
        self.client_cert_pem = Some(client_cert_pem.as_bytes().to_vec());
        self.client_key_pem = Some(client_key_pem.as_bytes().to_vec());

        self
    }

    pub fn with_insecure(mut self, insecure: bool) -> Self {
        self.insecure = insecure;

        self
    }

    pub fn build(self) -> Result<TLSConfig, RvError> {
        let provider = rustls::crypto::CryptoProvider::get_default()
            .cloned()
            .unwrap_or(Arc::new(rustls::crypto::ring::default_provider()));

        let builder = ClientConfig::builder_with_provider(provider.clone())
            .with_protocol_versions(ALL_VERSIONS)
            .expect("all TLS versions");

        let builder = if self.insecure {
            log::debug!("Certificate verification disabled");
            builder
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(DisabledVerifier))
        } else if let Some(server_ca) = &self.server_ca_pem {
            let mut cert_reader = BufReader::new(&server_ca[..]);
            let root_certs =
                rustls_pemfile::certs(&mut cert_reader).collect::<Result<Vec<_>, _>>()?;

            let mut root_store = RootCertStore::empty();
            let (_added, _ignored) = root_store.add_parsable_certificates(root_certs);
            builder.with_root_certificates(root_store)
        } else {
            let root_store = RootCertStore {
                roots: TLS_SERVER_ROOTS.to_vec(),
            };
            builder.with_root_certificates(root_store)
        };

        let client_config = if let (Some(client_cert_pem), Some(client_key_pem)) =
            (&self.client_cert_pem, &self.client_key_pem)
        {
            let mut cert_reader = BufReader::new(&client_cert_pem[..]);
            let client_certs =
                rustls_pemfile::certs(&mut cert_reader).collect::<Result<Vec<_>, _>>()?;
            let client_key = PrivateKeyDer::from_pem_slice(client_key_pem)?;

            builder.with_client_auth_cert(client_certs, client_key)?
        } else {
            builder.with_no_client_auth()
        };
        Ok(TLSConfig {
            client_config,
            server_ca_pem: self.server_ca_pem,
            client_cert_pem: self.client_cert_pem,
            client_key_pem: self.client_key_pem,
            insecure: self.insecure,
        })
    }
}

impl TLSConfig {
    pub fn client_config(&self) -> &ClientConfig {
        &self.client_config
    }

    pub fn clone_inner(&self) -> ClientConfig {
        self.client_config.clone()
    }

    pub fn server_ca_pem(&self) -> Option<&[u8]> {
        self.server_ca_pem.as_deref()
    }

    pub fn client_cert_pem(&self) -> Option<&[u8]> {
        self.client_cert_pem.as_deref()
    }

    pub fn client_key_pem(&self) -> Option<&[u8]> {
        self.client_key_pem.as_deref()
    }

    pub fn insecure(&self) -> bool {
        self.insecure
    }
}
