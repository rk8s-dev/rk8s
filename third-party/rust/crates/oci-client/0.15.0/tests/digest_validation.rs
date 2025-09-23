// Tests for validating digests of different types and for malicious servers
use std::net::SocketAddr;

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::get,
    Router,
};
use oci_client::{
    client::{linux_amd64_resolver, ClientConfig, ClientProtocol},
    Client, Reference,
};
use sha2::{Digest, Sha256, Sha512};
use tokio::{net::TcpListener, task::JoinHandle};

const DIGEST_HEADER: &str = "Docker-Content-Digest";

static MANIFEST: &[u8] = include_bytes!("./fixtures/manifest.json");
static BLOB: &[u8] = include_bytes!("./fixtures/blob.tar.gz");
static CONFIG: &[u8] = include_bytes!("./fixtures/config.json");

lazy_static::lazy_static! {
    static ref MANIFEST_DIGEST: String = digest(MANIFEST);
    static ref MANIFEST_DIGEST_SHA512: String = digest_sha512(MANIFEST);
    static ref BLOB_DIGEST: String = digest(BLOB);
    static ref BLOB_DIGEST_SHA512: String = digest_sha512(BLOB);
    static ref CONFIG_DIGEST: String = digest(CONFIG);
    static ref CONFIG_DIGEST_SHA512: String = digest_sha512(CONFIG);
}

fn digest(data: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(data))
}

fn digest_sha512(data: &[u8]) -> String {
    format!("sha512:{:x}", Sha512::digest(data))
}

async fn manifest_handler(
    State(state): State<ServerConfig>,
    Path(digest): Path<String>,
) -> (HeaderMap, &'static [u8]) {
    let resp_digest = if digest.starts_with("sha256:") && state.bad_manifest {
        digest
    } else {
        MANIFEST_DIGEST.clone()
    };

    let mut headers = HeaderMap::new();
    headers.insert(DIGEST_HEADER, resp_digest.parse().unwrap());
    headers.insert(
        "Content-Type",
        "application/vnd.docker.distribution.manifest.v2+json"
            .parse()
            .unwrap(),
    );

    (headers, MANIFEST)
}

async fn blob_handler(
    State(state): State<ServerConfig>,
    Path(digest): Path<String>,
) -> Result<(HeaderMap, &'static [u8]), StatusCode> {
    let (content, resp_digest) = match digest.as_str() {
        d if d == CONFIG_DIGEST.as_str() => (
            CONFIG,
            if state.bad_config {
                "sha256:deadbeef"
            } else {
                CONFIG_DIGEST.as_str()
            },
        ),
        d if state.blob_sha512 && d == BLOB_DIGEST.as_str() => (
            BLOB,
            if state.bad_blob {
                "sha256:deadbeef"
            } else {
                BLOB_DIGEST_SHA512.as_str()
            },
        ),
        d if d == BLOB_DIGEST.as_str() => (
            BLOB,
            if state.bad_blob {
                "sha256:deadbeef"
            } else {
                BLOB_DIGEST.as_str()
            },
        ),
        _ => return Err(StatusCode::NOT_FOUND),
    };

    let mut headers = HeaderMap::new();
    headers.insert(DIGEST_HEADER, resp_digest.parse().unwrap());

    Ok((headers, content))
}

#[derive(Clone, Copy)]
struct ServerConfig {
    bad_manifest: bool,
    bad_config: bool,
    bad_blob: bool,
    blob_sha512: bool,
}

struct BadServer {
    handle: JoinHandle<()>,
    pub server: String,
}

impl Drop for BadServer {
    fn drop(&mut self) {
        self.handle.abort()
    }
}

impl BadServer {
    pub async fn new(config: ServerConfig) -> Self {
        let app = Router::new()
            .route("/v2/busybox/manifests/{digest}", get(manifest_handler))
            .route("/v2/busybox/blobs/{digest}", get(blob_handler))
            .with_state(config);

        let addr = SocketAddr::from(([127, 0, 0, 1], 0));
        let listener = TcpListener::bind(addr).await.unwrap();
        let server_addr = listener.local_addr().unwrap();
        let port = server_addr.port();
        let server = format!("127.0.0.1:{}", port);
        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self { handle, server }
    }
}
#[tokio::test]
async fn test_bad_manifest() {
    let server = BadServer::new(ServerConfig {
        bad_manifest: true,
        bad_config: false,
        bad_blob: false,
        blob_sha512: false,
    })
    .await;

    let client = Client::new(ClientConfig {
        protocol: ClientProtocol::Http,
        platform_resolver: Some(Box::new(linux_amd64_resolver)),
        ..Default::default()
    });
    let auth = &oci_client::secrets::RegistryAuth::Anonymous;

    let reference = Reference::try_from(format!(
        "{}/busybox@sha256:deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
        server.server
    ))
    .expect("failed to parse reference");

    client
        .pull_manifest(&reference, auth)
        .await
        .expect_err("Expected an error with a mismatched sha");
}

#[tokio::test]
async fn test_bad_config() {
    let server = BadServer::new(ServerConfig {
        bad_manifest: false,
        bad_config: true,
        bad_blob: false,
        blob_sha512: false,
    })
    .await;

    let client = Client::new(ClientConfig {
        protocol: ClientProtocol::Http,
        platform_resolver: Some(Box::new(linux_amd64_resolver)),
        ..Default::default()
    });
    let auth = &oci_client::secrets::RegistryAuth::Anonymous;

    let reference = Reference::try_from(format!(
        "{}/busybox@{}",
        server.server,
        MANIFEST_DIGEST.as_str()
    ))
    .expect("failed to parse reference");

    assert!(
        client
            .pull(
                &reference,
                auth,
                vec!["application/vnd.docker.image.rootfs.diff.tar.gzip"],
            )
            .await
            .is_err(),
        "Expected an error with a bad config"
    );
}

#[tokio::test]
async fn test_bad_blob() {
    let server = BadServer::new(ServerConfig {
        bad_manifest: false,
        bad_config: false,
        bad_blob: true,
        blob_sha512: false,
    })
    .await;
    let client = Client::new(ClientConfig {
        protocol: ClientProtocol::Http,
        platform_resolver: Some(Box::new(linux_amd64_resolver)),
        ..Default::default()
    });
    let auth = &oci_client::secrets::RegistryAuth::Anonymous;

    let reference = Reference::try_from(format!(
        "{}/busybox@{}",
        server.server,
        MANIFEST_DIGEST.as_str()
    ))
    .expect("failed to parse reference");

    assert!(
        client
            .pull(
                &reference,
                auth,
                vec!["application/vnd.docker.image.rootfs.diff.tar.gzip"],
            )
            .await
            .is_err(),
        "Expected an error with a bad blob"
    );
}

#[tokio::test]
async fn test_good_pull() {
    let server = BadServer::new(ServerConfig {
        bad_manifest: false,
        bad_config: false,
        bad_blob: false,
        blob_sha512: false,
    })
    .await;

    let client = Client::new(ClientConfig {
        protocol: ClientProtocol::Http,
        platform_resolver: Some(Box::new(linux_amd64_resolver)),
        ..Default::default()
    });
    let auth = &oci_client::secrets::RegistryAuth::Anonymous;

    let reference = Reference::try_from(format!(
        "{}/busybox@{}",
        server.server,
        MANIFEST_DIGEST.as_str()
    ))
    .expect("failed to parse reference");

    client
        .pull(
            &reference,
            auth,
            vec!["application/vnd.docker.image.rootfs.diff.tar.gzip"],
        )
        .await
        .expect("Expected a good pull");
}

#[tokio::test]
async fn test_different_reference_sha() {
    let server = BadServer::new(ServerConfig {
        bad_manifest: false,
        bad_config: false,
        bad_blob: false,
        blob_sha512: false,
    })
    .await;

    let client = Client::new(ClientConfig {
        protocol: ClientProtocol::Http,
        platform_resolver: Some(Box::new(linux_amd64_resolver)),
        ..Default::default()
    });
    let auth = &oci_client::secrets::RegistryAuth::Anonymous;

    let reference = Reference::try_from(format!(
        "{}/busybox@sha256:deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
        server.server
    ))
    .expect("failed to parse reference");

    client
        .pull_manifest(&reference, auth)
        .await
        .expect_err("Expected an error with a mismatched reference sha");

    // Also try using a sha512 digest
    let reference = Reference::try_from(format!(
        "{}/busybox@sha256:deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
        server.server
    ))
    .expect("failed to parse reference");

    client
        .pull_manifest(&reference, auth)
        .await
        .expect_err("Expected an error with a mismatched reference sha");
}

#[tokio::test]
async fn test_different_manifest_algos() {
    let server = BadServer::new(ServerConfig {
        bad_manifest: false,
        bad_config: false,
        bad_blob: false,
        blob_sha512: false,
    })
    .await;
    let client = Client::new(ClientConfig {
        protocol: ClientProtocol::Http,
        platform_resolver: Some(Box::new(linux_amd64_resolver)),
        ..Default::default()
    });
    let auth = &oci_client::secrets::RegistryAuth::Anonymous;
    let reference = Reference::try_from(format!(
        "{}/busybox@{}",
        server.server,
        MANIFEST_DIGEST_SHA512.as_str()
    ))
    .expect("failed to parse reference");

    client
        .pull_manifest(&reference, auth)
        .await
        .expect("Expected a good pull with two different algorithms");
}

#[tokio::test]
async fn test_different_blob_algos() {
    let server = BadServer::new(ServerConfig {
        bad_manifest: false,
        bad_config: false,
        bad_blob: false,
        blob_sha512: true,
    })
    .await;

    let client = Client::new(ClientConfig {
        protocol: ClientProtocol::Http,
        platform_resolver: Some(Box::new(linux_amd64_resolver)),
        ..Default::default()
    });
    let auth = &oci_client::secrets::RegistryAuth::Anonymous;

    let reference = Reference::try_from(format!(
        "{}/busybox@{}",
        server.server,
        MANIFEST_DIGEST.as_str()
    ))
    .expect("failed to parse reference");

    client
        .pull(
            &reference,
            auth,
            vec!["application/vnd.docker.image.rootfs.diff.tar.gzip"],
        )
        .await
        .expect("Expected a good pull");
}
