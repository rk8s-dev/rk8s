use std::time::Duration;

use super::backoff::{BackOffStatus, BackOffWhenFail};
use http::{Request, Uri};
use hyper_openssl::client::legacy::HttpsConnector;
use hyper_util::client::legacy::connect::HttpConnector;
use openssl::{
    error::ErrorStack,
    pkey::PKey,
    ssl::{SslConnector, SslMethod},
    x509::X509,
};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::{channel::Change, Channel, Endpoint};
use tower::Service;
use tower::{balance::p2c::Balance, buffer::Buffer, discover::Change as TowerChange};

use crate::error::Result;

pub type SslConnectorBuilder = openssl::ssl::SslConnectorBuilder;
pub type OpenSslResult<T> = std::result::Result<T, ErrorStack>;
// Below are some type alias for make clearer types.
pub type TonicRequest = Request<tonic::body::Body>;
pub type Balanced<T> = Balance<T, TonicRequest>;
pub type OpenSslChannel =
    Buffer<TonicRequest, <Balanced<OpenSslDiscover<Uri>> as Service<TonicRequest>>::Future>;
/// OpenSslDiscover is the backend for balanced channel based on OpenSSL transports.
/// Because `Channel::balance` doesn't allow us to provide custom connector, we must implement ourselves' balancer...
pub type OpenSslDiscover<K> = ReceiverStream<Result<TowerChange<K, BackOffWhenFail<Channel>>>>;

#[derive(Clone)]
pub struct OpenSslConnector(HttpsConnector<HttpConnector>);

impl std::fmt::Debug for OpenSslConnector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("OpenSslConnector").finish()
    }
}

impl OpenSslConnector {
    pub fn create_default() -> OpenSslResult<Self> {
        let conf = OpenSslClientConfig::default();
        conf.build()
    }
}

#[cfg(feature = "tls")]
compile_error!(concat!(
    "**You should only enable one of `tls` and `tls-openssl`.** Reason: ",
    "For now, `tls-openssl` would take over the transport layer (sockets) to implement TLS based connection. ",
    "As a result, once using with `tonic`'s internal TLS implementation (which based on `rustls`), ", 
    "we may create TLS tunnels over TLS tunnels or directly fail because of some sorts of misconfiguration.")
);

/// Create a balanced channel using the OpenSSL config.
pub fn balanced_channel(
    connector: OpenSslConnector,
) -> Result<(OpenSslChannel, Sender<Change<Uri, Endpoint>>)> {
    let (tx, rx) = tokio::sync::mpsc::channel(16);
    let tls_conn = create_openssl_discover(connector, rx);
    let balance = Balance::new(tls_conn);
    // Note: the buffer should already be configured when creating the internal channels,
    // we wrap this in the buffer is just for making them `Clone`.
    let buffered = Buffer::new(balance, 1024);

    Ok((buffered, tx))
}

/// Create a connector which dials TLS connections by openssl.
fn create_openssl_connector(builder: SslConnectorBuilder) -> OpenSslResult<OpenSslConnector> {
    let mut http = HttpConnector::new();
    http.enforce_http(false);
    let https = HttpsConnector::with_connector(http, builder)?;
    Ok(OpenSslConnector(https))
}

/// Create a discover which mapping Endpoints into SSL connections.
/// Because this would fully take over the transport layer by a security channel,
/// you should NOT enable `tonic/ssl` feature (or tonic may try to create SSL session over the security transport...).
fn create_openssl_discover<K: Send + 'static>(
    connector: OpenSslConnector,
    mut incoming: Receiver<Change<K, Endpoint>>,
) -> OpenSslDiscover<K> {
    let (tx, rx) = tokio::sync::mpsc::channel(16);
    let fut = async move {
        while let Some(x) = incoming.recv().await {
            let r = async {
                match x {
                    Change::Insert(name, e) => {
                        let chan = e.connect_with_connector_lazy(connector.clone().0);
                        Ok(TowerChange::Insert(
                            name,
                            BackOffWhenFail::new(
                                chan,
                                BackOffStatus::new(
                                    /*initial*/ Duration::from_secs(1),
                                    /*max*/ Duration::from_secs(256),
                                ),
                            ),
                        ))
                    }
                    Change::Remove(name) => Ok(TowerChange::Remove(name)),
                }
            }
            .await;
            if tx.send(r).await.is_err() {
                return;
            }
        }
    };
    tokio::task::spawn(fut);
    ReceiverStream::new(rx)
}

/// The configuration type for a openssl connection.
/// For best flexibility, we are making it a callback over `SslConnectorBuilder`.
/// Which allows users to fine-tweaking the detail of the SSL connection.
/// This isn't `Clone` due to the implementation.
pub struct OpenSslClientConfig(OpenSslResult<SslConnectorBuilder>);

impl Default for OpenSslClientConfig {
    fn default() -> Self {
        let get_builder = || {
            let mut b = SslConnector::builder(SslMethod::tls_client())?;
            // It seems gRPC doesn't support upgrade to HTTP/2,
            // if we haven't specified the protocol by ALPN, it would return a `GONE`.
            // "h2" is the ALPN name for HTTP/2, see:
            // https://www.iana.org/assignments/tls-extensiontype-values/tls-extensiontype-values.xhtml#alpn-protocol-ids
            b.set_alpn_protos(b"\x02h2")?;
            OpenSslResult::Ok(b)
        };
        Self(get_builder())
    }
}

impl std::fmt::Debug for OpenSslClientConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("OpenSslClientConfig")
            .field(&"callbacks")
            .finish()
    }
}

impl OpenSslClientConfig {
    /// Manually modify the SslConnectorBuilder by a pure function.
    pub fn manually(self, f: impl FnOnce(&mut SslConnectorBuilder) -> OpenSslResult<()>) -> Self {
        Self(self.0.and_then(|mut b| {
            f(&mut b)?;
            Ok(b)
        }))
    }

    /// Add a CA into the cert storage via the binary of the PEM file of CA cert.
    /// If the argument is empty, do nothing.
    /// Please also note unlike the gRPC implementation of Google, this doesn't allow
    /// partial chain of certificates by default.
    /// If you want to provide a non-root CA here, please manually allow partial chain here.
    /// ```no_run
    /// use openssl::x509::verify::X509VerifyFlags;
    /// manually(|c| c.cert_store_mut().set_flags(X509VerifyFlags::PARTIAL_CHAIN))
    /// ```
    pub fn ca_cert_pem(self, s: &[u8]) -> Self {
        if s.is_empty() {
            return self;
        }
        self.manually(move |cb| {
            let ca = X509::from_pem(s)?;
            cb.cert_store_mut().add_cert(ca)?;
            Ok(())
        })
    }

    /// Add a client cert for the request.
    /// If any of the argument is empty, do nothing.
    /// Also note this would only **load the first certificate** in the PEM file.
    /// If you need to load a client certificate bundle (a client cert + some mid CA certs)
    /// you may need to add those certificates manually. (Which is the default behavior
    /// of Google's gRPC implementation.)
    /// ```no_run
    /// use openssl::x509::X509;
    /// manually(|c| {
    ///     let mut client_certs = X509::stack_from_pem(&cert_pem)?;
    ///     if client_certs.len() > 1 {
    ///         for i in client_certs.drain(1..) {
    ///             c.add_extra_chain_cert(i)?;
    ///         }
    ///     }
    /// })
    /// ```
    pub fn client_cert_pem_and_key(self, cert_pem: &[u8], key_pem: &[u8]) -> Self {
        if cert_pem.is_empty() || key_pem.is_empty() {
            return self;
        }
        self.manually(|cb| {
            let client = X509::from_pem(cert_pem)?;
            let client_key = PKey::private_key_from_pem(key_pem)?;
            cb.set_certificate(&client)?;
            cb.set_private_key(&client_key)?;
            Ok(())
        })
    }

    pub(crate) fn build(self) -> OpenSslResult<OpenSslConnector> {
        self.0.and_then(create_openssl_connector)
    }
}
