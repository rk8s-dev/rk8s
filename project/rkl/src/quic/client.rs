use crate::commands::pod::TLSConnectionArgs;
use crate::quic::client::private::Sealed;
use crate::quic::verifier::SkipServerVerification;
use common::RksMessage;
use common::quic::RksConnection;
use derive_more::Deref;
use libvault::modules::pki::types::{IssueCertificateRequest, IssueCertificateResponse};
use quinn::crypto::rustls::QuicClientConfig;
use quinn::{ClientConfig, Endpoint};
use rustls::RootCertStore;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Duration;
use tokio::time;
use tonic::async_trait;
use tracing::{debug, info, warn};

const DEFAULT_TTL: &str = "64h";

mod private {
    pub trait Sealed {}
}

#[async_trait]
pub trait ClientType: Sealed + Sized {
    async fn pre_init(_client: &QUICClient<Self>) -> anyhow::Result<()> {
        Ok(())
    }
}

pub struct Cli;

impl Sealed for Cli {}

impl Sealed for Daemon {}

#[async_trait]
impl ClientType for Cli {
    async fn pre_init(client: &QUICClient<Self>) -> anyhow::Result<()> {
        client
            .conn
            .send_msg(&RksMessage::UserRequest("hello".to_string()))
            .await
    }
}

pub struct Daemon;

#[async_trait]
impl ClientType for Daemon {}

#[derive(Deref)]
pub struct QUICClient<Type> {
    #[deref]
    conn: RksConnection,
    _marker: PhantomData<Type>,
}

impl<Type> Clone for QUICClient<Type> {
    fn clone(&self) -> Self {
        Self {
            conn: self.conn.clone(),
            _marker: PhantomData,
        }
    }
}

impl<Type: ClientType + Send> QUICClient<Type> {
    pub async fn connect(
        addr: impl AsRef<str>,
        tls_cfg: &TLSConnectionArgs,
    ) -> anyhow::Result<Self> {
        let addr = addr.as_ref();
        info!(target: "rkl::quic", server_addr = %addr, "initializing QUIC client");

        let config = build_no_certificates_config()?;
        debug!(target: "rkl::quic", server_addr = %addr, "establishing provisional connection");
        let mut conn = try_connect(addr, Some(config)).await?;

        if tls_cfg.enable_tls {
            let join_token = tls_cfg.join_token.as_deref().unwrap_or_default();
            let config = Self::request_certificate(conn, join_token).await?;

            debug!(target: "rkl::quic", server_addr = %addr, "provisional connection succeeded; retrying with client certificate");
            conn = try_connect(addr, Some(config)).await?;
        }

        let client = Self {
            conn,
            _marker: PhantomData,
        };

        debug!(target: "rkl::quic", server_addr = %addr, "running client pre-initialization hook");
        Type::pre_init(&client).await?;

        info!(target: "rkl::quic", server_addr = %addr, "QUIC client ready");
        Ok(client)
    }

    async fn request_certificate(
        conn: RksConnection,
        join_token: impl Into<String>,
    ) -> anyhow::Result<ClientConfig> {
        let request = IssueCertificateRequest {
            common_name: Some("rkl-cluster".to_string()),
            alt_names: Some("rkl.svc.cluster.local".to_string()),
            ip_sans: None,
            ttl: Some(DEFAULT_TTL.to_string()),
        };

        debug!(target: "rkl::quic", "sending certificate signing request");
        conn.send_msg(&RksMessage::CertificateSign {
            req: request,
            token: join_token.into(),
        })
        .await?;

        debug!(target: "rkl::quic", "awaiting certificate signing response");
        let res = match conn.fetch_msg().await? {
            RksMessage::Certificate(res) => res,
            x => anyhow::bail!("expected certificate response, but got {x}"),
        };
        debug!(target: "rkl::quic", "received certificate signing response");

        let IssuedCertMaterial {
            certs,
            trust_roots,
            private_key,
        } = into_cert_material(res)?;

        let rustls_config = rustls::ClientConfig::builder()
            .with_root_certificates(trust_roots.clone())
            .with_client_auth_cert(certs, private_key)?;

        let quic_crypto = QuicClientConfig::try_from(rustls_config)?;
        Ok(quinn::ClientConfig::new(Arc::new(quic_crypto)))
    }
}

fn build_no_certificates_config() -> anyhow::Result<quinn::ClientConfig> {
    let mut tls = rustls::ClientConfig::builder()
        .with_root_certificates(RootCertStore::empty())
        .with_no_client_auth();
    tls.dangerous()
        .set_certificate_verifier(Arc::new(SkipServerVerification));

    let quic_crypto = QuicClientConfig::try_from(tls)?;
    Ok(quinn::ClientConfig::new(Arc::new(quic_crypto)))
}

fn trust_roots_from_certs(certs: &[CertificateDer<'static>]) -> anyhow::Result<Arc<RootCertStore>> {
    let mut roots = RootCertStore::empty();
    for cert in certs.iter().skip(1) {
        roots.add(cert.clone())?;
    }
    Ok(Arc::new(roots))
}

struct IssuedCertMaterial {
    certs: Vec<CertificateDer<'static>>,
    trust_roots: Arc<RootCertStore>,
    private_key: PrivateKeyDer<'static>,
}

fn into_cert_material(resp: IssueCertificateResponse) -> anyhow::Result<IssuedCertMaterial> {
    let certs = resp.to_certs()?;
    let trust_roots = trust_roots_from_certs(&certs)?;
    let private_key = PrivateKeyDer::from_pem_slice(resp.private_key.as_bytes())?;

    Ok(IssuedCertMaterial {
        certs,
        trust_roots,
        private_key,
    })
}

async fn try_connect(
    addr: impl AsRef<str>,
    config: Option<ClientConfig>,
) -> anyhow::Result<RksConnection> {
    let addr = addr.as_ref();

    let mut endpoint = Endpoint::client("0.0.0.0:0".parse()?)?;
    if let Some(config) = config {
        endpoint.set_default_client_config(config);
    }

    let mut attempts = 0u32;
    let conn = loop {
        attempts += 1;
        match endpoint.connect(addr.parse()?, "rks.svc.cluster.local") {
            Ok(connecting) => match connecting.await {
                Ok(conn) => {
                    info!(
                        target: "rkl::quic",
                        server_addr = %addr,
                        attempts,
                        "QUIC connection established"
                    );
                    break conn;
                }
                Err(e) => {
                    warn!(
                        target: "rkl::quic",
                        server_addr = %addr,
                        attempts,
                        error = %e,
                        "QUIC handshake failed; retrying"
                    );
                    time::sleep(Duration::from_secs(2)).await;
                }
            },
            Err(e) => {
                warn!(
                    target: "rkl::quic",
                    server_addr = %addr,
                    attempts,
                    error = %e,
                    "endpoint connect error; retrying"
                );
                time::sleep(Duration::from_secs(2)).await;
            }
        }
    };
    Ok(RksConnection::new(conn))
}
