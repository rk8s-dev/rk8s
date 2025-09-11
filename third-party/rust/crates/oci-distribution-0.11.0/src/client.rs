//! OCI distribution client
//!
//! *Note*: This client is very feature poor. We hope to expand this to be a complete
//! OCI distribution client in the future.

use crate::config::ConfigFile;
use crate::errors::*;
use crate::manifest::{
    ImageIndexEntry, OciDescriptor, OciImageIndex, OciImageManifest, OciManifest, Versioned,
    IMAGE_CONFIG_MEDIA_TYPE, IMAGE_LAYER_GZIP_MEDIA_TYPE, IMAGE_LAYER_MEDIA_TYPE,
    IMAGE_MANIFEST_LIST_MEDIA_TYPE, IMAGE_MANIFEST_MEDIA_TYPE, OCI_IMAGE_INDEX_MEDIA_TYPE,
    OCI_IMAGE_MEDIA_TYPE,
};
use crate::secrets::RegistryAuth;
use crate::secrets::*;
use crate::sha256_digest;
use crate::Reference;

use crate::errors::{OciDistributionError, Result};
use crate::token_cache::{RegistryOperation, RegistryToken, RegistryTokenType, TokenCache};
use futures_util::future;
use futures_util::stream::{self, StreamExt, TryStreamExt};
use futures_util::Stream;
use http::HeaderValue;
use http_auth::{parser::ChallengeParser, ChallengeRef};
use olpc_cjson::CanonicalFormatter;
use reqwest::header::HeaderMap;
use reqwest::{RequestBuilder, Url};
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::sync::Arc;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::sync::RwLock;
use tracing::{debug, trace, warn};

const MIME_TYPES_DISTRIBUTION_MANIFEST: &[&str] = &[
    IMAGE_MANIFEST_MEDIA_TYPE,
    IMAGE_MANIFEST_LIST_MEDIA_TYPE,
    OCI_IMAGE_MEDIA_TYPE,
    OCI_IMAGE_INDEX_MEDIA_TYPE,
];

const PUSH_CHUNK_MAX_SIZE: usize = 4096 * 1024;

/// Default value for `ClientConfig::max_concurrent_upload`
pub const DEFAULT_MAX_CONCURRENT_UPLOAD: usize = 16;

/// Default value for `ClientConfig::max_concurrent_download`
pub const DEFAULT_MAX_CONCURRENT_DOWNLOAD: usize = 16;

/// The data for an image or module.
#[derive(Clone)]
pub struct ImageData {
    /// The layers of the image or module.
    pub layers: Vec<ImageLayer>,
    /// The digest of the image or module.
    pub digest: Option<String>,
    /// The Configuration object of the image or module.
    pub config: Config,
    /// The manifest of the image or module.
    pub manifest: Option<OciImageManifest>,
}

/// The data returned by an OCI registry after a successful push
/// operation is completed
pub struct PushResponse {
    /// Pullable url for the config
    pub config_url: String,
    /// Pullable url for the manifest
    pub manifest_url: String,
}

/// The data returned by a successful tags/list Request
#[derive(Deserialize, Debug)]
pub struct TagResponse {
    /// Repository Name
    pub name: String,
    /// List of existing Tags
    pub tags: Vec<String>,
}

/// The data and media type for an image layer
#[derive(Clone)]
pub struct ImageLayer {
    /// The data of this layer
    pub data: Vec<u8>,
    /// The media type of this layer
    pub media_type: String,
    /// This OPTIONAL property contains arbitrary metadata for this descriptor.
    /// This OPTIONAL property MUST use the [annotation rules](https://github.com/opencontainers/image-spec/blob/main/annotations.md#rules)
    pub annotations: Option<HashMap<String, String>>,
}

impl ImageLayer {
    /// Constructs a new ImageLayer struct with provided data and media type
    pub fn new(
        data: Vec<u8>,
        media_type: String,
        annotations: Option<HashMap<String, String>>,
    ) -> Self {
        ImageLayer {
            data,
            media_type,
            annotations,
        }
    }

    /// Constructs a new ImageLayer struct with provided data and
    /// media type application/vnd.oci.image.layer.v1.tar
    pub fn oci_v1(data: Vec<u8>, annotations: Option<HashMap<String, String>>) -> Self {
        Self::new(data, IMAGE_LAYER_MEDIA_TYPE.to_string(), annotations)
    }
    /// Constructs a new ImageLayer struct with provided data and
    /// media type application/vnd.oci.image.layer.v1.tar+gzip
    pub fn oci_v1_gzip(data: Vec<u8>, annotations: Option<HashMap<String, String>>) -> Self {
        Self::new(data, IMAGE_LAYER_GZIP_MEDIA_TYPE.to_string(), annotations)
    }

    /// Helper function to compute the sha256 digest of an image layer
    pub fn sha256_digest(&self) -> String {
        sha256_digest(&self.data)
    }
}

/// The data and media type for a configuration object
#[derive(Clone)]
pub struct Config {
    /// The data of this config object
    pub data: Vec<u8>,
    /// The media type of this object
    pub media_type: String,
    /// This OPTIONAL property contains arbitrary metadata for this descriptor.
    /// This OPTIONAL property MUST use the [annotation rules](https://github.com/opencontainers/image-spec/blob/main/annotations.md#rules)
    pub annotations: Option<HashMap<String, String>>,
}

impl Config {
    /// Constructs a new Config struct with provided data and media type
    pub fn new(
        data: Vec<u8>,
        media_type: String,
        annotations: Option<HashMap<String, String>>,
    ) -> Self {
        Config {
            data,
            media_type,
            annotations,
        }
    }

    /// Constructs a new Config struct with provided data and
    /// media type application/vnd.oci.image.config.v1+json
    pub fn oci_v1(data: Vec<u8>, annotations: Option<HashMap<String, String>>) -> Self {
        Self::new(data, IMAGE_CONFIG_MEDIA_TYPE.to_string(), annotations)
    }

    /// Construct a new Config struct with provided [`ConfigFile`] and
    /// media type `application/vnd.oci.image.config.v1+json`
    pub fn oci_v1_from_config_file(
        config_file: ConfigFile,
        annotations: Option<HashMap<String, String>>,
    ) -> Result<Self> {
        let data = serde_json::to_vec(&config_file)?;
        Ok(Self::new(
            data,
            IMAGE_CONFIG_MEDIA_TYPE.to_string(),
            annotations,
        ))
    }

    /// Helper function to compute the sha256 digest of this config object
    pub fn sha256_digest(&self) -> String {
        sha256_digest(&self.data)
    }
}

impl TryFrom<Config> for ConfigFile {
    type Error = crate::errors::OciDistributionError;

    fn try_from(config: Config) -> Result<Self> {
        let config = String::from_utf8(config.data)
            .map_err(|e| OciDistributionError::ConfigConversionError(e.to_string()))?;
        let config_file: ConfigFile = serde_json::from_str(&config)
            .map_err(|e| OciDistributionError::ConfigConversionError(e.to_string()))?;
        Ok(config_file)
    }
}

/// The OCI client connects to an OCI registry and fetches OCI images.
///
/// An OCI registry is a container registry that adheres to the OCI Distribution
/// specification. DockerHub is one example, as are ACR and GCR. This client
/// provides a native Rust implementation for pulling OCI images.
///
/// Some OCI registries support completely anonymous access. But most require
/// at least an Oauth2 handshake. Typlically, you will want to create a new
/// client, and then run the `auth()` method, which will attempt to get
/// a read-only bearer token. From there, pulling images can be done with
/// the `pull_*` functions.
///
/// For true anonymous access, you can skip `auth()`. This is not recommended
/// unless you are sure that the remote registry does not require Oauth2.
#[derive(Clone)]
pub struct Client {
    config: Arc<ClientConfig>,
    // Registry -> RegistryAuth
    auth_store: Arc<RwLock<HashMap<String, RegistryAuth>>>,
    tokens: TokenCache,
    client: reqwest::Client,
    push_chunk_size: usize,
}

impl Default for Client {
    fn default() -> Self {
        Self {
            config: Arc::default(),
            auth_store: Arc::default(),
            tokens: TokenCache::default(),
            client: reqwest::Client::default(),
            push_chunk_size: PUSH_CHUNK_MAX_SIZE,
        }
    }
}

/// A source that can provide a `ClientConfig`.
/// If you are using this crate in your own application, you can implement this
/// trait on your configuration type so that it can be passed to `Client::from_source`.
pub trait ClientConfigSource {
    /// Provides a `ClientConfig`.
    fn client_config(&self) -> ClientConfig;
}

impl TryFrom<ClientConfig> for Client {
    type Error = OciDistributionError;

    fn try_from(config: ClientConfig) -> std::result::Result<Self, Self::Error> {
        #[allow(unused_mut)]
        let mut client_builder = reqwest::Client::builder();
        #[cfg(not(target_arch = "wasm32"))]
        let mut client_builder =
            client_builder.danger_accept_invalid_certs(config.accept_invalid_certificates);

        client_builder = match () {
            #[cfg(all(feature = "native-tls", not(target_arch = "wasm32")))]
            () => client_builder.danger_accept_invalid_hostnames(config.accept_invalid_hostnames),
            #[cfg(any(not(feature = "native-tls"), target_arch = "wasm32"))]
            () => client_builder,
        };

        #[cfg(not(target_arch = "wasm32"))]
        for c in &config.extra_root_certificates {
            let cert = match c.encoding {
                CertificateEncoding::Der => reqwest::Certificate::from_der(c.data.as_slice())?,
                CertificateEncoding::Pem => reqwest::Certificate::from_pem(c.data.as_slice())?,
            };
            client_builder = client_builder.add_root_certificate(cert);
        }

        Ok(Self {
            config: Arc::new(config),
            client: client_builder.build()?,
            push_chunk_size: PUSH_CHUNK_MAX_SIZE,
            ..Default::default()
        })
    }
}

impl Client {
    /// Create a new client with the supplied config
    pub fn new(config: ClientConfig) -> Self {
        Client::try_from(config).unwrap_or_else(|err| {
            warn!("Cannot create OCI client from config: {:?}", err);
            warn!("Creating client with default configuration");
            Self {
                push_chunk_size: PUSH_CHUNK_MAX_SIZE,
                ..Default::default()
            }
        })
    }

    /// Create a new client with the supplied config
    pub fn from_source(config_source: &impl ClientConfigSource) -> Self {
        Self::new(config_source.client_config())
    }

    async fn store_auth(&self, registry: &str, auth: RegistryAuth) {
        self.auth_store
            .write()
            .await
            .insert(registry.to_string(), auth);
    }

    async fn is_stored_auth(&self, registry: &str) -> bool {
        self.auth_store.read().await.contains_key(registry)
    }

    async fn store_auth_if_needed(&self, registry: &str, auth: &RegistryAuth) {
        if !self.is_stored_auth(registry).await {
            self.store_auth(registry, auth.clone()).await;
        }
    }

    /// Checks if we got a token, if we don't - create it and store it in cache.
    async fn get_auth_token(
        &self,
        reference: &Reference,
        op: RegistryOperation,
    ) -> Option<RegistryTokenType> {
        let registry = reference.resolve_registry();
        let auth = self.auth_store.read().await.get(registry)?.clone();
        match self.tokens.get(reference, op).await {
            Some(token) => Some(token),
            None => {
                let token = self._auth(reference, &auth, op).await.ok()??;
                self.tokens.insert(reference, op, token.clone()).await;
                Some(token)
            }
        }
    }

    /// Fetches the available Tags for the given Reference
    ///
    /// The client will check if it's already been authenticated and if
    /// not will attempt to do.
    pub async fn list_tags(
        &self,
        image: &Reference,
        auth: &RegistryAuth,
        n: Option<usize>,
        last: Option<&str>,
    ) -> Result<TagResponse> {
        let op = RegistryOperation::Pull;
        let url = self.to_list_tags_url(image);

        self.store_auth_if_needed(image.resolve_registry(), auth)
            .await;

        let request = self.client.get(&url);
        let request = if let Some(num) = n {
            request.query(&[("n", num)])
        } else {
            request
        };
        let request = if let Some(l) = last {
            request.query(&[("last", l)])
        } else {
            request
        };
        let request = RequestBuilderWrapper {
            client: self,
            request_builder: request,
        };
        let res = request
            .apply_auth(image, op)
            .await?
            .into_request_builder()
            .send()
            .await?;
        let status = res.status();
        let body = res.bytes().await?;

        validate_registry_response(status, &body, &url)?;

        Ok(serde_json::from_str(std::str::from_utf8(&body)?)?)
    }

    /// Pull an image and return the bytes
    ///
    /// The client will check if it's already been authenticated and if
    /// not will attempt to do.
    pub async fn pull(
        &self,
        image: &Reference,
        auth: &RegistryAuth,
        accepted_media_types: Vec<&str>,
    ) -> Result<ImageData> {
        debug!("Pulling image: {:?}", image);
        self.store_auth_if_needed(image.resolve_registry(), auth)
            .await;

        let (manifest, digest, config) = self._pull_manifest_and_config(image).await?;

        self.validate_layers(&manifest, accepted_media_types)
            .await?;

        let layers = stream::iter(&manifest.layers)
            .map(|layer| {
                // This avoids moving `self` which is &Self
                // into the async block. We only want to capture
                // as &Self
                let this = &self;
                async move {
                    let mut out: Vec<u8> = Vec::new();
                    debug!("Pulling image layer");
                    this.pull_blob(image, layer, &mut out).await?;
                    Ok::<_, OciDistributionError>(ImageLayer::new(
                        out,
                        layer.media_type.clone(),
                        layer.annotations.clone(),
                    ))
                }
            })
            .boxed() // Workaround to rustc issue https://github.com/rust-lang/rust/issues/104382
            .buffer_unordered(self.config.max_concurrent_download)
            .try_collect()
            .await?;

        Ok(ImageData {
            layers,
            manifest: Some(manifest),
            config,
            digest: Some(digest),
        })
    }

    /// Push an image and return the uploaded URL of the image
    ///
    /// The client will check if it's already been authenticated and if
    /// not will attempt to do.
    ///
    /// If a manifest is not provided, the client will attempt to generate
    /// it from the provided image and config data.
    ///
    /// Returns pullable URL for the image
    pub async fn push(
        &self,
        image_ref: &Reference,
        layers: &[ImageLayer],
        config: Config,
        auth: &RegistryAuth,
        manifest: Option<OciImageManifest>,
    ) -> Result<PushResponse> {
        debug!("Pushing image: {:?}", image_ref);
        self.store_auth_if_needed(image_ref.resolve_registry(), auth)
            .await;

        let manifest: OciImageManifest = match manifest {
            Some(m) => m,
            None => OciImageManifest::build(layers, &config, None),
        };

        // Upload layers
        stream::iter(layers)
            .map(|layer| {
                // This avoids moving `self` which is &Self
                // into the async block. We only want to capture
                // as &Self
                let this = &self;
                async move {
                    let digest = layer.sha256_digest();
                    this.push_blob(image_ref, &layer.data, &digest).await?;
                    Result::Ok(())
                }
            })
            .boxed() // Workaround to rustc issue https://github.com/rust-lang/rust/issues/104382
            .buffer_unordered(self.config.max_concurrent_upload)
            .try_for_each(future::ok)
            .await?;

        let config_url = self
            .push_blob(image_ref, &config.data, &manifest.config.digest)
            .await?;
        let manifest_url = self.push_manifest(image_ref, &manifest.into()).await?;

        Ok(PushResponse {
            config_url,
            manifest_url,
        })
    }

    /// Pushes a blob to the registry
    pub async fn push_blob(
        &self,
        image_ref: &Reference,
        data: &[u8],
        digest: &str,
    ) -> Result<String> {
        match self.push_blob_chunked(image_ref, data, digest).await {
            Ok(url) => Ok(url),
            Err(OciDistributionError::SpecViolationError(violation)) => {
                warn!(?violation, "Registry is not respecting the OCI Distribution Specification when doing chunked push operations");
                warn!("Attempting monolithic push");
                self.push_blob_monolithically(image_ref, data, digest).await
            }
            Err(e) => Err(e),
        }
    }

    /// Pushes a blob to the registry as a monolith
    ///
    /// Returns the pullable location of the blob
    async fn push_blob_monolithically(
        &self,
        image: &Reference,
        blob_data: &[u8],
        blob_digest: &str,
    ) -> Result<String> {
        let location = self.begin_push_monolithical_session(image).await?;
        self.push_monolithically(&location, image, blob_data, blob_digest)
            .await
    }

    /// Pushes a blob to the registry as a series of chunks
    ///
    /// Returns the pullable location of the blob
    async fn push_blob_chunked(
        &self,
        image: &Reference,
        blob_data: &[u8],
        blob_digest: &str,
    ) -> Result<String> {
        let mut location = self.begin_push_chunked_session(image).await?;
        let mut start: usize = 0;
        loop {
            (location, start) = self.push_chunk(&location, image, blob_data, start).await?;
            if start >= blob_data.len() {
                break;
            }
        }
        self.end_push_chunked_session(&location, image, blob_digest)
            .await
    }

    /// Perform an OAuth v2 auth request if necessary.
    ///
    /// This performs authorization and then stores the token internally to be used
    /// on other requests.
    pub async fn auth(
        &self,
        image: &Reference,
        authentication: &RegistryAuth,
        operation: RegistryOperation,
    ) -> Result<Option<String>> {
        self.store_auth_if_needed(image.resolve_registry(), authentication)
            .await;
        // preserve old caching behavior
        match self._auth(image, authentication, operation).await {
            Ok(Some(RegistryTokenType::Bearer(token))) => {
                self.tokens
                    .insert(image, operation, RegistryTokenType::Bearer(token.clone()))
                    .await;
                Ok(Some(token.token().to_string()))
            }
            Ok(Some(RegistryTokenType::Basic(username, password))) => {
                self.tokens
                    .insert(
                        image,
                        operation,
                        RegistryTokenType::Basic(username, password),
                    )
                    .await;
                Ok(None)
            }
            Ok(None) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Internal auth that retrieves token.
    async fn _auth(
        &self,
        image: &Reference,
        authentication: &RegistryAuth,
        operation: RegistryOperation,
    ) -> Result<Option<RegistryTokenType>> {
        debug!("Authorizing for image: {:?}", image);
        // The version request will tell us where to go.
        let url = format!(
            "{}://{}/v2/",
            self.config.protocol.scheme_for(image.resolve_registry()),
            image.resolve_registry()
        );
        debug!(?url);
        let res = self.client.get(&url).send().await?;
        let dist_hdr = match res.headers().get(reqwest::header::WWW_AUTHENTICATE) {
            Some(h) => h,
            None => return Ok(None),
        };

        let challenge = match BearerChallenge::try_from(dist_hdr) {
            Ok(c) => c,
            Err(e) => {
                debug!(error = ?e, "Falling back to HTTP Basic Auth");
                if let RegistryAuth::Basic(username, password) = authentication {
                    return Ok(Some(RegistryTokenType::Basic(
                        username.to_string(),
                        password.to_string(),
                    )));
                }
                return Ok(None);
            }
        };

        // Allow for either push or pull authentication
        let scope = match operation {
            RegistryOperation::Pull => format!("repository:{}:pull", image.repository()),
            RegistryOperation::Push => format!("repository:{}:pull,push", image.repository()),
        };

        let realm = challenge.realm.as_ref();
        let service = challenge.service.as_ref();
        let mut query = vec![("scope", &scope)];

        if let Some(s) = service {
            query.push(("service", s))
        }

        // TODO: At some point in the future, we should support sending a secret to the
        // server for auth. This particular workflow is for read-only public auth.
        debug!(?realm, ?service, ?scope, "Making authentication call");

        let auth_res = self
            .client
            .get(realm)
            .query(&query)
            .apply_authentication(authentication)
            .send()
            .await?;

        match auth_res.status() {
            reqwest::StatusCode::OK => {
                let text = auth_res.text().await?;
                debug!("Received response from auth request: {}", text);
                let token: RegistryToken = serde_json::from_str(&text)
                    .map_err(|e| OciDistributionError::RegistryTokenDecodeError(e.to_string()))?;
                debug!("Successfully authorized for image '{:?}'", image);
                Ok(Some(RegistryTokenType::Bearer(token)))
            }
            _ => {
                let reason = auth_res.text().await?;
                debug!("Failed to authenticate for image '{:?}': {}", image, reason);
                Err(OciDistributionError::AuthenticationFailure(reason))
            }
        }
    }

    /// Fetch a manifest's digest from the remote OCI Distribution service.
    ///
    /// If the connection has already gone through authentication, this will
    /// use the bearer token. Otherwise, this will attempt an anonymous pull.
    ///
    /// Will first attempt to read the `Docker-Content-Digest` header using a
    /// HEAD request. If this header is not present, will make a second GET
    /// request and return the SHA256 of the response body.
    pub async fn fetch_manifest_digest(
        &self,
        image: &Reference,
        auth: &RegistryAuth,
    ) -> Result<String> {
        self.store_auth_if_needed(image.resolve_registry(), auth)
            .await;

        let url = self.to_v2_manifest_url(image);
        debug!("HEAD image manifest from {}", url);
        let res = RequestBuilderWrapper::from_client(self, |client| client.head(&url))
            .apply_accept(MIME_TYPES_DISTRIBUTION_MANIFEST)?
            .apply_auth(image, RegistryOperation::Pull)
            .await?
            .into_request_builder()
            .send()
            .await?;

        trace!(headers=?res.headers(), "Got Headers");
        if res.headers().get("Docker-Content-Digest").is_none() {
            debug!("GET image manifest from {}", url);
            let res = RequestBuilderWrapper::from_client(self, |client| client.get(&url))
                .apply_accept(MIME_TYPES_DISTRIBUTION_MANIFEST)?
                .apply_auth(image, RegistryOperation::Pull)
                .await?
                .into_request_builder()
                .send()
                .await?;
            let status = res.status();
            let headers = res.headers().clone();
            trace!(headers=?res.headers(), "Got Headers");
            let body = res.bytes().await?;
            validate_registry_response(status, &body, &url)?;

            digest_header_value(headers, Some(&body))
        } else {
            let status = res.status();
            let headers = res.headers().clone();
            let body = res.bytes().await?;
            validate_registry_response(status, &body, &url)?;

            digest_header_value(headers, None)
        }
    }

    async fn validate_layers(
        &self,
        manifest: &OciImageManifest,
        accepted_media_types: Vec<&str>,
    ) -> Result<()> {
        if manifest.layers.is_empty() {
            return Err(OciDistributionError::PullNoLayersError);
        }

        for layer in &manifest.layers {
            if !accepted_media_types.iter().any(|i| i.eq(&layer.media_type)) {
                return Err(OciDistributionError::IncompatibleLayerMediaTypeError(
                    layer.media_type.clone(),
                ));
            }
        }

        Ok(())
    }

    /// Pull a manifest from the remote OCI Distribution service.
    ///
    /// The client will check if it's already been authenticated and if
    /// not will attempt to do.
    ///
    /// A Tuple is returned containing the [OciImageManifest](crate::manifest::OciImageManifest)
    /// and the manifest content digest hash.
    ///
    /// If a multi-platform Image Index manifest is encountered, a platform-specific
    /// Image manifest will be selected using the client's default platform resolution.
    pub async fn pull_image_manifest(
        &self,
        image: &Reference,
        auth: &RegistryAuth,
    ) -> Result<(OciImageManifest, String)> {
        self.store_auth_if_needed(image.resolve_registry(), auth)
            .await;

        self._pull_image_manifest(image).await
    }

    /// Pull a manifest from the remote OCI Distribution service without parsing it.
    ///
    /// The client will check if it's already been authenticated and if
    /// not will attempt to do.
    ///
    /// A Tuple is returned containing raw byte representation of the manifest
    /// and the manifest content digest.
    pub async fn pull_manifest_raw(
        &self,
        image: &Reference,
        auth: &RegistryAuth,
        accepted_media_types: &[&str],
    ) -> Result<(Vec<u8>, String)> {
        self.store_auth_if_needed(image.resolve_registry(), auth)
            .await;

        self._pull_manifest_raw(image, accepted_media_types).await
    }

    /// Pull a manifest from the remote OCI Distribution service.
    ///
    /// The client will check if it's already been authenticated and if
    /// not will attempt to do.
    ///
    /// A Tuple is returned containing the [Manifest](crate::manifest::OciImageManifest)
    /// and the manifest content digest hash.
    pub async fn pull_manifest(
        &self,
        image: &Reference,
        auth: &RegistryAuth,
    ) -> Result<(OciManifest, String)> {
        self.store_auth_if_needed(image.resolve_registry(), auth)
            .await;

        self._pull_manifest(image).await
    }

    /// Pull an image manifest from the remote OCI Distribution service.
    ///
    /// If the connection has already gone through authentication, this will
    /// use the bearer token. Otherwise, this will attempt an anonymous pull.
    ///
    /// If a multi-platform Image Index manifest is encountered, a platform-specific
    /// Image manifest will be selected using the client's default platform resolution.
    async fn _pull_image_manifest(&self, image: &Reference) -> Result<(OciImageManifest, String)> {
        let (manifest, digest) = self._pull_manifest(image).await?;
        match manifest {
            OciManifest::Image(image_manifest) => Ok((image_manifest, digest)),
            OciManifest::ImageIndex(image_index_manifest) => {
                debug!("Inspecting Image Index Manifest");
                let digest = if let Some(resolver) = &self.config.platform_resolver {
                    resolver(&image_index_manifest.manifests)
                } else {
                    return Err(OciDistributionError::ImageIndexParsingNoPlatformResolverError);
                };

                match digest {
                    Some(digest) => {
                        debug!("Selected manifest entry with digest: {}", digest);
                        let manifest_entry_reference = Reference::with_digest(
                            image.registry().to_string(),
                            image.repository().to_string(),
                            digest.clone(),
                        );
                        self._pull_manifest(&manifest_entry_reference)
                            .await
                            .and_then(|(manifest, _digest)| match manifest {
                                OciManifest::Image(manifest) => Ok((manifest, digest)),
                                OciManifest::ImageIndex(_) => {
                                    Err(OciDistributionError::ImageManifestNotFoundError(
                                        "received Image Index manifest instead".to_string(),
                                    ))
                                }
                            })
                    }
                    None => Err(OciDistributionError::ImageManifestNotFoundError(
                        "no entry found in image index manifest matching client's default platform"
                            .to_string(),
                    )),
                }
            }
        }
    }

    /// Pull a manifest from the remote OCI Distribution service without parsing it.
    ///
    /// If the connection has already gone through authentication, this will
    /// use the bearer token. Otherwise, this will attempt an anonymous pull.
    async fn _pull_manifest_raw(
        &self,
        image: &Reference,
        accepted_media_types: &[&str],
    ) -> Result<(Vec<u8>, String)> {
        let url = self.to_v2_manifest_url(image);
        debug!("Pulling image manifest from {}", url);

        let res = RequestBuilderWrapper::from_client(self, |client| client.get(&url))
            .apply_accept(accepted_media_types)?
            .apply_auth(image, RegistryOperation::Pull)
            .await?
            .into_request_builder()
            .send()
            .await?;
        let headers = res.headers().clone();
        let status = res.status();
        let body = res.bytes().await?;

        validate_registry_response(status, &body, &url)?;

        let digest = digest_header_value(headers, Some(&body))?;

        Ok((body.to_vec(), digest))
    }

    /// Pull a manifest from the remote OCI Distribution service.
    ///
    /// If the connection has already gone through authentication, this will
    /// use the bearer token. Otherwise, this will attempt an anonymous pull.
    async fn _pull_manifest(&self, image: &Reference) -> Result<(OciManifest, String)> {
        let (body, digest) = self
            ._pull_manifest_raw(image, MIME_TYPES_DISTRIBUTION_MANIFEST)
            .await?;

        let text = std::str::from_utf8(&body)?;

        self.validate_image_manifest(text).await?;

        debug!("Parsing response as Manifest: {}", &text);
        let manifest = serde_json::from_str(text)
            .map_err(|e| OciDistributionError::ManifestParsingError(e.to_string()))?;
        Ok((manifest, digest))
    }

    async fn validate_image_manifest(&self, text: &str) -> Result<()> {
        debug!("validating manifest: {}", text);
        let versioned: Versioned = serde_json::from_str(text)
            .map_err(|e| OciDistributionError::VersionedParsingError(e.to_string()))?;
        if versioned.schema_version != 2 {
            return Err(OciDistributionError::UnsupportedSchemaVersionError(
                versioned.schema_version,
            ));
        }
        if let Some(media_type) = versioned.media_type {
            if media_type != IMAGE_MANIFEST_MEDIA_TYPE
                && media_type != OCI_IMAGE_MEDIA_TYPE
                && media_type != IMAGE_MANIFEST_LIST_MEDIA_TYPE
                && media_type != OCI_IMAGE_INDEX_MEDIA_TYPE
            {
                return Err(OciDistributionError::UnsupportedMediaTypeError(media_type));
            }
        }

        Ok(())
    }

    /// Pull a manifest and its config from the remote OCI Distribution service.
    ///
    /// The client will check if it's already been authenticated and if
    /// not will attempt to do.
    ///
    /// A Tuple is returned containing the [OciImageManifest](crate::manifest::OciImageManifest),
    /// the manifest content digest hash and the contents of the manifests config layer
    /// as a String.
    pub async fn pull_manifest_and_config(
        &self,
        image: &Reference,
        auth: &RegistryAuth,
    ) -> Result<(OciImageManifest, String, String)> {
        self.store_auth_if_needed(image.resolve_registry(), auth)
            .await;

        self._pull_manifest_and_config(image)
            .await
            .and_then(|(manifest, digest, config)| {
                Ok((
                    manifest,
                    digest,
                    String::from_utf8(config.data).map_err(|e| {
                        OciDistributionError::GenericError(Some(format!(
                            "Cannot not UTF8 compliant: {}",
                            e
                        )))
                    })?,
                ))
            })
    }

    async fn _pull_manifest_and_config(
        &self,
        image: &Reference,
    ) -> Result<(OciImageManifest, String, Config)> {
        let (manifest, digest) = self._pull_image_manifest(image).await?;

        let mut out: Vec<u8> = Vec::new();
        debug!("Pulling config layer");
        self.pull_blob(image, &manifest.config, &mut out).await?;
        let media_type = manifest.config.media_type.clone();
        let annotations = manifest.annotations.clone();
        Ok((manifest, digest, Config::new(out, media_type, annotations)))
    }

    /// Push a manifest list to an OCI registry.
    ///
    /// This pushes a manifest list to an OCI registry.
    pub async fn push_manifest_list(
        &self,
        reference: &Reference,
        auth: &RegistryAuth,
        manifest: OciImageIndex,
    ) -> Result<String> {
        self.store_auth_if_needed(reference.resolve_registry(), auth)
            .await;
        self.push_manifest(reference, &OciManifest::ImageIndex(manifest))
            .await
    }

    /// Pull a single layer from an OCI registry.
    ///
    /// This pulls the layer for a particular image that is identified by
    /// the given layer descriptor. The image reference is used to find the
    /// repository and the registry, but it is not used to verify that
    /// the digest is a layer inside of the image. (The manifest is
    /// used for that.)
    pub async fn pull_blob<T: AsyncWrite + Unpin>(
        &self,
        image: &Reference,
        layer: &OciDescriptor,
        mut out: T,
    ) -> Result<()> {
        let url = self.to_v2_blob_url(image.resolve_registry(), image.repository(), &layer.digest);

        let mut response = RequestBuilderWrapper::from_client(self, |client| client.get(&url))
            .apply_accept(MIME_TYPES_DISTRIBUTION_MANIFEST)?
            .apply_auth(image, RegistryOperation::Pull)
            .await?
            .into_request_builder()
            .send()
            .await?;

        if let Some(urls) = &layer.urls {
            for url in urls {
                if response.error_for_status_ref().is_ok() {
                    break;
                }

                let url = Url::parse(url)
                    .map_err(|e| OciDistributionError::UrlParseError(e.to_string()))?;

                if url.scheme() == "http" || url.scheme() == "https" {
                    // NOTE: we must not authenticate on additional URLs as those
                    // can be abused to leak credentials or tokens.  Please
                    // refer to CVE-2020-15157 for more information.
                    response =
                        RequestBuilderWrapper::from_client(self, |client| client.get(url.clone()))
                            .apply_accept(MIME_TYPES_DISTRIBUTION_MANIFEST)?
                            .into_request_builder()
                            .send()
                            .await?
                }
            }
        }

        let mut stream = response.error_for_status()?.bytes_stream();

        while let Some(bytes) = stream.next().await {
            out.write_all(&bytes?).await?;
        }

        Ok(())
    }

    /// Stream a single layer from an OCI registry.
    ///
    /// This is a streaming version of [`Client::pull_blob`].
    /// Returns [`Stream`](futures_util::Stream).
    pub async fn pull_blob_stream(
        &self,
        image: &Reference,
        layer: &OciDescriptor,
    ) -> Result<impl Stream<Item = std::result::Result<bytes::Bytes, std::io::Error>>> {
        let url = self.to_v2_blob_url(image.resolve_registry(), image.repository(), &layer.digest);

        let mut response = RequestBuilderWrapper::from_client(self, |client| client.get(&url))
            .apply_accept(MIME_TYPES_DISTRIBUTION_MANIFEST)?
            .apply_auth(image, RegistryOperation::Pull)
            .await?
            .into_request_builder()
            .send()
            .await?;

        if let Some(urls) = &layer.urls {
            for url in urls {
                if response.error_for_status_ref().is_ok() {
                    break;
                }

                let url = Url::parse(url)
                    .map_err(|e| OciDistributionError::UrlParseError(e.to_string()))?;

                if url.scheme() == "http" || url.scheme() == "https" {
                    // NOTE: we must not authenticate on additional URLs as those
                    // can be abused to leak credentials or tokens.  Please
                    // refer to CVE-2020-15157 for more information.
                    response =
                        RequestBuilderWrapper::from_client(self, |client| client.get(url.clone()))
                            .apply_accept(MIME_TYPES_DISTRIBUTION_MANIFEST)?
                            .into_request_builder()
                            .send()
                            .await?
                }
            }
        }

        let stream = response
            .error_for_status()?
            .bytes_stream()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e));

        Ok(stream)
    }

    /// Begins a session to push an image to registry in a monolithical way
    ///
    /// Returns URL with session UUID
    async fn begin_push_monolithical_session(&self, image: &Reference) -> Result<String> {
        let url = &self.to_v2_blob_upload_url(image);
        debug!(?url, "begin_push_monolithical_session");
        let res = RequestBuilderWrapper::from_client(self, |client| client.post(url))
            .apply_auth(image, RegistryOperation::Push)
            .await?
            .into_request_builder()
            .send()
            .await?;

        // OCI spec requires the status code be 202 Accepted to successfully begin the push process
        self.extract_location_header(image, res, &reqwest::StatusCode::ACCEPTED)
            .await
    }

    /// Begins a session to push an image to registry as a series of chunks
    ///
    /// Returns URL with session UUID
    async fn begin_push_chunked_session(&self, image: &Reference) -> Result<String> {
        let url = &self.to_v2_blob_upload_url(image);
        debug!(?url, "begin_push_session");
        let res = RequestBuilderWrapper::from_client(self, |client| client.post(url))
            .apply_auth(image, RegistryOperation::Push)
            .await?
            .into_request_builder()
            .header("Content-Length", 0)
            .send()
            .await?;

        // OCI spec requires the status code be 202 Accepted to successfully begin the push process
        self.extract_location_header(image, res, &reqwest::StatusCode::ACCEPTED)
            .await
    }

    /// Closes the chunked push session
    ///
    /// Returns the pullable URL for the image
    async fn end_push_chunked_session(
        &self,
        location: &str,
        image: &Reference,
        digest: &str,
    ) -> Result<String> {
        let url = Url::parse_with_params(location, &[("digest", digest)])
            .map_err(|e| OciDistributionError::GenericError(Some(e.to_string())))?;
        let res = RequestBuilderWrapper::from_client(self, |client| client.put(url.clone()))
            .apply_auth(image, RegistryOperation::Push)
            .await?
            .into_request_builder()
            .header("Content-Length", 0)
            .send()
            .await?;
        self.extract_location_header(image, res, &reqwest::StatusCode::CREATED)
            .await
    }

    /// Pushes a layer to a registry as a monolithical blob.
    ///
    /// Returns the URL location for the next layer
    async fn push_monolithically(
        &self,
        location: &str,
        image: &Reference,
        layer: &[u8],
        blob_digest: &str,
    ) -> Result<String> {
        let mut url = Url::parse(location).unwrap();
        url.query_pairs_mut().append_pair("digest", blob_digest);
        let url = url.to_string();

        debug!(size = layer.len(), location = ?url, "Pushing monolithically");
        if layer.is_empty() {
            return Err(OciDistributionError::PushNoDataError);
        };
        let mut headers = HeaderMap::new();
        headers.insert(
            "Content-Length",
            format!("{}", layer.len()).parse().unwrap(),
        );
        headers.insert("Content-Type", "application/octet-stream".parse().unwrap());

        let res = RequestBuilderWrapper::from_client(self, |client| client.put(&url))
            .apply_auth(image, RegistryOperation::Push)
            .await?
            .into_request_builder()
            .headers(headers)
            .body(layer.to_vec())
            .send()
            .await?;

        // Returns location
        self.extract_location_header(image, res, &reqwest::StatusCode::CREATED)
            .await
    }

    /// Pushes a single chunk of a blob to a registry,
    /// as part of a chunked blob upload.
    ///
    /// Returns the URL location for the next chunk
    async fn push_chunk(
        &self,
        location: &str,
        image: &Reference,
        blob_data: &[u8],
        start_byte: usize,
    ) -> Result<(String, usize)> {
        if blob_data.is_empty() {
            return Err(OciDistributionError::PushNoDataError);
        };
        let end_byte = if (start_byte + self.push_chunk_size) < blob_data.len() {
            start_byte + self.push_chunk_size - 1
        } else {
            blob_data.len() - 1
        };
        let body = blob_data[start_byte..end_byte + 1].to_vec();
        let mut headers = HeaderMap::new();
        headers.insert(
            "Content-Range",
            format!("{}-{}", start_byte, end_byte).parse().unwrap(),
        );
        headers.insert("Content-Length", format!("{}", body.len()).parse().unwrap());
        headers.insert("Content-Type", "application/octet-stream".parse().unwrap());

        debug!(
            ?start_byte,
            ?end_byte,
            blob_data_len = blob_data.len(),
            body_len = body.len(),
            ?location,
            ?headers,
            "Pushing chunk"
        );

        let res = RequestBuilderWrapper::from_client(self, |client| client.patch(location))
            .apply_auth(image, RegistryOperation::Push)
            .await?
            .into_request_builder()
            .headers(headers)
            .body(body)
            .send()
            .await?;

        // Returns location for next chunk and the start byte for the next range
        Ok((
            self.extract_location_header(image, res, &reqwest::StatusCode::ACCEPTED)
                .await?,
            end_byte + 1,
        ))
    }

    /// Mounts a blob to the provided reference, from the given source
    pub async fn mount_blob(
        &self,
        image: &Reference,
        source: &Reference,
        digest: &str,
    ) -> Result<()> {
        let base_url = self.to_v2_blob_upload_url(image);
        let url = Url::parse_with_params(
            &base_url,
            &[("mount", digest), ("from", source.repository())],
        )
        .map_err(|e| OciDistributionError::UrlParseError(e.to_string()))?;

        let res = RequestBuilderWrapper::from_client(self, |client| client.post(url.clone()))
            .apply_auth(image, RegistryOperation::Push)
            .await?
            .into_request_builder()
            .send()
            .await?;

        self.extract_location_header(image, res, &reqwest::StatusCode::CREATED)
            .await?;

        Ok(())
    }

    /// Pushes the manifest for a specified image
    ///
    /// Returns pullable manifest URL
    pub async fn push_manifest(&self, image: &Reference, manifest: &OciManifest) -> Result<String> {
        let mut headers = HeaderMap::new();
        let content_type = manifest.content_type();
        headers.insert("Content-Type", content_type.parse().unwrap());

        // Serialize the manifest with a canonical json formatter, as described at
        // https://github.com/opencontainers/image-spec/blob/main/considerations.md#json
        let mut body = Vec::new();
        let mut ser = serde_json::Serializer::with_formatter(&mut body, CanonicalFormatter::new());
        manifest.serialize(&mut ser).unwrap();

        self.push_manifest_raw(image, body, manifest.content_type().parse().unwrap())
            .await
    }

    /// Pushes the manifest, provided as raw bytes, for a specified image
    ///
    /// Returns pullable manifest url
    pub async fn push_manifest_raw(
        &self,
        image: &Reference,
        body: Vec<u8>,
        content_type: HeaderValue,
    ) -> Result<String> {
        let url = self.to_v2_manifest_url(image);
        debug!(?url, ?content_type, "push manifest");

        let mut headers = HeaderMap::new();
        headers.insert("Content-Type", content_type);

        // Calculate the digest of the manifest, this is useful
        // if the remote registry is violating the OCI Distribution Specification.
        // See below for more details.
        let manifest_hash = sha256_digest(&body);

        let res = RequestBuilderWrapper::from_client(self, |client| client.put(url.clone()))
            .apply_auth(image, RegistryOperation::Push)
            .await?
            .into_request_builder()
            .headers(headers)
            .body(body)
            .send()
            .await?;

        let ret = self
            .extract_location_header(image, res, &reqwest::StatusCode::CREATED)
            .await;

        if matches!(ret, Err(OciDistributionError::RegistryNoLocationError)) {
            // The registry is violating the OCI Distribution Spec, BUT the OCI
            // image/artifact has been uploaded successfully.
            // The `Location` header contains the sha256 digest of the manifest,
            // we can reuse the value we calculated before.
            // The workaround is there because repositories such as
            // AWS ECR are violating this aspect of the spec. This at least let the
            // oci-distribution users interact with these registries.
            warn!("Registry is not respecting the OCI Distribution Specification: it didn't return the Location of the uploaded Manifest inside of the response headers. Working around this issue...");

            let url_base = url
                .strip_suffix(image.tag().unwrap_or("latest"))
                .expect("The manifest URL always ends with the image tag suffix");
            let url_by_digest = format!("{}{}", url_base, manifest_hash);

            return Ok(url_by_digest);
        }

        ret
    }

    async fn extract_location_header(
        &self,
        image: &Reference,
        res: reqwest::Response,
        expected_status: &reqwest::StatusCode,
    ) -> Result<String> {
        debug!(expected_status_code=?expected_status.as_u16(),
            status_code=?res.status().as_u16(),
            "extract location header");
        if res.status().eq(expected_status) {
            let location_header = res.headers().get("Location");
            debug!(location=?location_header, "Location header");
            match location_header {
                None => Err(OciDistributionError::RegistryNoLocationError),
                Some(lh) => self.location_header_to_url(image, lh),
            }
        } else if res.status().is_success() && expected_status.is_success() {
            Err(OciDistributionError::SpecViolationError(format!(
                "Expected HTTP Status {}, got {} instead",
                expected_status,
                res.status(),
            )))
        } else {
            let url = res.url().to_string();
            let code = res.status().as_u16();
            let message = res.text().await?;
            Err(OciDistributionError::ServerError { url, code, message })
        }
    }

    /// Helper function to convert location header to URL
    ///
    /// Location may be absolute (containing the protocol and/or hostname), or relative (containing just the URL path)
    /// Returns a properly formatted absolute URL
    fn location_header_to_url(
        &self,
        image: &Reference,
        location_header: &reqwest::header::HeaderValue,
    ) -> Result<String> {
        let lh = location_header.to_str()?;
        if lh.starts_with("/v2/") {
            Ok(format!(
                "{}://{}{}",
                self.config.protocol.scheme_for(image.resolve_registry()),
                image.resolve_registry(),
                lh
            ))
        } else {
            Ok(lh.to_string())
        }
    }

    /// Convert a Reference to a v2 manifest URL.
    fn to_v2_manifest_url(&self, reference: &Reference) -> String {
        if let Some(digest) = reference.digest() {
            format!(
                "{}://{}/v2/{}/manifests/{}",
                self.config
                    .protocol
                    .scheme_for(reference.resolve_registry()),
                reference.resolve_registry(),
                reference.repository(),
                digest,
            )
        } else {
            format!(
                "{}://{}/v2/{}/manifests/{}",
                self.config
                    .protocol
                    .scheme_for(reference.resolve_registry()),
                reference.resolve_registry(),
                reference.repository(),
                reference.tag().unwrap_or("latest")
            )
        }
    }

    /// Convert a Reference to a v2 blob (layer) URL.
    fn to_v2_blob_url(&self, registry: &str, repository: &str, digest: &str) -> String {
        format!(
            "{}://{}/v2/{}/blobs/{}",
            self.config.protocol.scheme_for(registry),
            registry,
            repository,
            digest,
        )
    }

    /// Convert a Reference to a v2 blob upload URL.
    fn to_v2_blob_upload_url(&self, reference: &Reference) -> String {
        self.to_v2_blob_url(
            reference.resolve_registry(),
            reference.repository(),
            "uploads/",
        )
    }

    fn to_list_tags_url(&self, reference: &Reference) -> String {
        format!(
            "{}://{}/v2/{}/tags/list",
            self.config
                .protocol
                .scheme_for(reference.resolve_registry()),
            reference.resolve_registry(),
            reference.repository(),
        )
    }
}

/// The OCI spec technically does not allow any codes but 200, 500, 401, and 404.
/// Obviously, HTTP servers are going to send other codes. This tries to catch the
/// obvious ones (200, 4XX, 5XX). Anything else is just treated as an error.
fn validate_registry_response(status: reqwest::StatusCode, body: &[u8], url: &str) -> Result<()> {
    match status {
        reqwest::StatusCode::OK => Ok(()),
        reqwest::StatusCode::UNAUTHORIZED => Err(OciDistributionError::UnauthorizedError {
            url: url.to_string(),
        }),
        s if s.is_success() => Err(OciDistributionError::SpecViolationError(format!(
            "Expected HTTP Status {}, got {} instead",
            reqwest::StatusCode::OK,
            status,
        ))),
        s if s.is_client_error() => {
            let text = std::str::from_utf8(body)?;
            // According to the OCI spec, we should see an error in the message body.
            let envelope = serde_json::from_str::<OciEnvelope>(text)?;
            Err(OciDistributionError::RegistryError {
                envelope,
                url: url.to_string(),
            })
        }
        s => {
            let text = std::str::from_utf8(body)?;

            Err(OciDistributionError::ServerError {
                code: s.as_u16(),
                url: url.to_string(),
                message: text.to_string(),
            })
        }
    }
}

/// The request builder wrapper allows to be instantiated from a
/// `Client` and allows composable operations on the request builder,
/// to produce a `RequestBuilder` object that can be executed.
struct RequestBuilderWrapper<'a> {
    client: &'a Client,
    request_builder: RequestBuilder,
}

// RequestBuilderWrapper type management
impl<'a> RequestBuilderWrapper<'a> {
    /// Create a `RequestBuilderWrapper` from a `Client` instance, by
    /// instantiating the internal `RequestBuilder` with the provided
    /// function `f`.
    fn from_client(
        client: &'a Client,
        f: impl Fn(&reqwest::Client) -> RequestBuilder,
    ) -> RequestBuilderWrapper {
        let request_builder = f(&client.client);
        RequestBuilderWrapper {
            client,
            request_builder,
        }
    }

    // Produces a final `RequestBuilder` out of this `RequestBuilderWrapper`
    fn into_request_builder(self) -> RequestBuilder {
        self.request_builder
    }
}

// Composable functions applicable to a `RequestBuilderWrapper`
impl<'a> RequestBuilderWrapper<'a> {
    fn apply_accept(&self, accept: &[&str]) -> Result<RequestBuilderWrapper> {
        let request_builder = self
            .request_builder
            .try_clone()
            .ok_or_else(|| {
                OciDistributionError::GenericError(Some(
                    "could not clone request builder".to_string(),
                ))
            })?
            .header("Accept", Vec::from(accept).join(", "));

        Ok(RequestBuilderWrapper {
            client: self.client,
            request_builder,
        })
    }

    /// Updates request as necessary for authentication.
    ///
    /// If the struct has Some(bearer), this will insert the bearer token in an
    /// Authorization header. It will also set the Accept header, which must
    /// be set on all OCI Registry requests. If the struct has HTTP Basic Auth
    /// credentials, these will be configured.
    async fn apply_auth(
        &self,
        image: &Reference,
        op: RegistryOperation,
    ) -> Result<RequestBuilderWrapper> {
        let mut headers = HeaderMap::new();

        if let Some(token) = self.client.get_auth_token(image, op).await {
            match token {
                RegistryTokenType::Bearer(token) => {
                    debug!("Using bearer token authentication.");
                    headers.insert("Authorization", token.bearer_token().parse().unwrap());
                }
                RegistryTokenType::Basic(username, password) => {
                    debug!("Using HTTP basic authentication.");
                    return Ok(RequestBuilderWrapper {
                        client: self.client,
                        request_builder: self
                            .request_builder
                            .try_clone()
                            .ok_or_else(|| {
                                OciDistributionError::GenericError(Some(
                                    "could not clone request builder".to_string(),
                                ))
                            })?
                            .headers(headers)
                            .basic_auth(username.to_string(), Some(password.to_string())),
                    });
                }
            }
        }
        Ok(RequestBuilderWrapper {
            client: self.client,
            request_builder: self
                .request_builder
                .try_clone()
                .ok_or_else(|| {
                    OciDistributionError::GenericError(Some(
                        "could not clone request builder".to_string(),
                    ))
                })?
                .headers(headers),
        })
    }
}

/// The encoding of the certificate
#[derive(Debug, Clone)]
pub enum CertificateEncoding {
    #[allow(missing_docs)]
    Der,
    #[allow(missing_docs)]
    Pem,
}

/// A x509 certificate
#[derive(Debug, Clone)]
pub struct Certificate {
    /// Which encoding is used by the certificate
    pub encoding: CertificateEncoding,

    /// Actual certificate
    pub data: Vec<u8>,
}

/// A client configuration
pub struct ClientConfig {
    /// Which protocol the client should use
    pub protocol: ClientProtocol,

    /// Accept invalid hostname. Defaults to false
    #[cfg(feature = "native-tls")]
    pub accept_invalid_hostnames: bool,

    /// Accept invalid certificates. Defaults to false
    pub accept_invalid_certificates: bool,

    /// A list of extra root certificate to trust. This can be used to connect
    /// to servers using self-signed certificates
    pub extra_root_certificates: Vec<Certificate>,

    /// A function that defines the client's behaviour if an Image Index Manifest
    /// (i.e Manifest List) is encountered when pulling an image.
    /// Defaults to [current_platform_resolver](self::current_platform_resolver),
    /// which attempts to choose an image matching the running OS and Arch.
    ///
    /// If set to None, an error is raised if an Image Index manifest is received
    /// during an image pull.
    pub platform_resolver: Option<Box<PlatformResolverFn>>,

    /// Maximum number of concurrent uploads to perform during a `push`
    /// operation.
    ///
    /// This defaults to [`DEFAULT_MAX_CONCURRENT_UPLOAD`].
    pub max_concurrent_upload: usize,

    /// Maximum number of concurrent downloads to perform during a `pull`
    /// operation.
    ///
    /// This defaults to [`DEFAULT_MAX_CONCURRENT_DOWNLOAD`].
    pub max_concurrent_download: usize,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            protocol: ClientProtocol::default(),
            #[cfg(feature = "native-tls")]
            accept_invalid_hostnames: false,
            accept_invalid_certificates: false,
            extra_root_certificates: Vec::new(),
            platform_resolver: Some(Box::new(current_platform_resolver)),
            max_concurrent_upload: DEFAULT_MAX_CONCURRENT_UPLOAD,
            max_concurrent_download: DEFAULT_MAX_CONCURRENT_DOWNLOAD,
        }
    }
}

// Be explicit about the traits supported by this type. This is needed to use
// the Client behind a dynamic reference.
// Something similar to what is described here: https://users.rust-lang.org/t/how-to-send-function-closure-to-another-thread/43549
type PlatformResolverFn = dyn Fn(&[ImageIndexEntry]) -> Option<String> + Send + Sync;

/// A platform resolver that chooses the first linux/amd64 variant, if present
pub fn linux_amd64_resolver(manifests: &[ImageIndexEntry]) -> Option<String> {
    manifests
        .iter()
        .find(|entry| {
            entry.platform.as_ref().map_or(false, |platform| {
                platform.os == "linux" && platform.architecture == "amd64"
            })
        })
        .map(|entry| entry.digest.clone())
}

/// A platform resolver that chooses the first windows/amd64 variant, if present
pub fn windows_amd64_resolver(manifests: &[ImageIndexEntry]) -> Option<String> {
    manifests
        .iter()
        .find(|entry| {
            entry.platform.as_ref().map_or(false, |platform| {
                platform.os == "windows" && platform.architecture == "amd64"
            })
        })
        .map(|entry| entry.digest.clone())
}

const MACOS: &str = "macos";
const DARWIN: &str = "darwin";

fn go_os() -> &'static str {
    // Massage Rust OS var to GO OS:
    // - Rust: https://doc.rust-lang.org/std/env/consts/constant.OS.html
    // - Go: https://golang.org/doc/install/source#environment
    match std::env::consts::OS {
        MACOS => DARWIN,
        other => other,
    }
}

const X86_64: &str = "x86_64";
const AMD64: &str = "amd64";
const X86: &str = "x86";
const AMD: &str = "amd";
const ARM64: &str = "arm64";
const AARCH64: &str = "aarch64";
const POWERPC64: &str = "powerpc64";
const PPC64LE: &str = "ppc64le";

fn go_arch() -> &'static str {
    // Massage Rust Architecture vars to GO equivalent:
    // - Rust: https://doc.rust-lang.org/std/env/consts/constant.ARCH.html
    // - Go: https://golang.org/doc/install/source#environment
    match std::env::consts::ARCH {
        X86_64 => AMD64,
        X86 => AMD,
        AARCH64 => ARM64,
        POWERPC64 => PPC64LE,
        other => other,
    }
}

/// A platform resolver that chooses the first variant matching the running OS/Arch, if present.
/// Doesn't currently handle platform.variants.
pub fn current_platform_resolver(manifests: &[ImageIndexEntry]) -> Option<String> {
    manifests
        .iter()
        .find(|entry| {
            entry.platform.as_ref().map_or(false, |platform| {
                platform.os == go_os() && platform.architecture == go_arch()
            })
        })
        .map(|entry| entry.digest.clone())
}

/// The protocol that the client should use to connect
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ClientProtocol {
    #[allow(missing_docs)]
    Http,
    #[allow(missing_docs)]
    #[default]
    Https,
    #[allow(missing_docs)]
    HttpsExcept(Vec<String>),
}

impl ClientProtocol {
    fn scheme_for(&self, registry: &str) -> &str {
        match self {
            ClientProtocol::Https => "https",
            ClientProtocol::Http => "http",
            ClientProtocol::HttpsExcept(exceptions) => {
                if exceptions.contains(&registry.to_owned()) {
                    "http"
                } else {
                    "https"
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
struct BearerChallenge {
    pub realm: Box<str>,
    pub service: Option<String>,
}

impl TryFrom<&HeaderValue> for BearerChallenge {
    type Error = String;

    fn try_from(value: &HeaderValue) -> std::result::Result<Self, Self::Error> {
        let parser = ChallengeParser::new(
            value
                .to_str()
                .map_err(|e| format!("cannot convert header value to string: {:?}", e))?,
        );
        parser
            .filter_map(|parser_res| {
                if let Ok(chalenge_ref) = parser_res {
                    let bearer_challenge = BearerChallenge::try_from(&chalenge_ref);
                    bearer_challenge.ok()
                } else {
                    None
                }
            })
            .next()
            .ok_or_else(|| "Cannot find Bearer challenge".to_string())
    }
}

impl TryFrom<&ChallengeRef<'_>> for BearerChallenge {
    type Error = String;

    fn try_from(value: &ChallengeRef<'_>) -> std::result::Result<Self, Self::Error> {
        if !value.scheme.eq_ignore_ascii_case("Bearer") {
            return Err(format!(
                "BearerChallenge doesn't support challenge scheme {:?}",
                value.scheme
            ));
        }
        let mut realm = None;
        let mut service = None;
        for (k, v) in &value.params {
            if k.eq_ignore_ascii_case("realm") {
                realm = Some(v.to_unescaped());
            }

            if k.eq_ignore_ascii_case("service") {
                service = Some(v.to_unescaped());
            }
        }

        let realm = realm.ok_or("missing required parameter realm")?;

        Ok(BearerChallenge {
            realm: realm.into_boxed_str(),
            service,
        })
    }
}

/// Extract `Docker-Content-Digest` header from manifest GET or HEAD request.
/// Can optionally supply a response body (i.e. the manifest itself) to
/// fallback to manually hashing this content. This should only be done if the
/// response body contains the image manifest.
fn digest_header_value(headers: HeaderMap, body: Option<&[u8]>) -> Result<String> {
    let digest_header = headers.get("Docker-Content-Digest");
    match digest_header {
        None => {
            if let Some(body) = body {
                // Fallback to hashing payload (tested with ECR)
                let digest = sha2::Sha256::digest(body);
                let hex = format!("sha256:{:x}", digest);
                debug!(%hex, "Computed digest of manifest payload.");
                Ok(hex)
            } else {
                Err(OciDistributionError::RegistryNoDigestError)
            }
        }
        Some(hv) => hv
            .to_str()
            .map(|s| s.to_string())
            .map_err(|e| OciDistributionError::GenericError(Some(e.to_string()))),
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::manifest::{self, IMAGE_DOCKER_LAYER_GZIP_MEDIA_TYPE};
    use std::convert::TryFrom;
    use std::fs;
    use std::path;
    use std::result::Result;
    use tempfile::TempDir;
    use tokio::io::AsyncReadExt;
    use tokio_util::io::StreamReader;

    #[cfg(feature = "test-registry")]
    use testcontainers::{clients, core::WaitFor, GenericImage};

    const HELLO_IMAGE_NO_TAG: &str = "webassembly.azurecr.io/hello-wasm";
    const HELLO_IMAGE_TAG: &str = "webassembly.azurecr.io/hello-wasm:v1";
    const HELLO_IMAGE_DIGEST: &str = "webassembly.azurecr.io/hello-wasm@sha256:51d9b231d5129e3ffc267c9d455c49d789bf3167b611a07ab6e4b3304c96b0e7";
    const HELLO_IMAGE_TAG_AND_DIGEST: &str = "webassembly.azurecr.io/hello-wasm:v1@sha256:51d9b231d5129e3ffc267c9d455c49d789bf3167b611a07ab6e4b3304c96b0e7";
    const TEST_IMAGES: &[&str] = &[
        // TODO(jlegrone): this image cannot be pulled currently because no `latest`
        //                 tag exists on the image repository. Re-enable this image
        //                 in tests once `latest` is published.
        // HELLO_IMAGE_NO_TAG,
        HELLO_IMAGE_TAG,
        HELLO_IMAGE_DIGEST,
        HELLO_IMAGE_TAG_AND_DIGEST,
    ];
    const GHCR_IO_IMAGE: &str = "ghcr.io/krustlet/oci-distribution/hello-wasm:v1";
    const DOCKER_IO_IMAGE: &str = "docker.io/library/hello-world@sha256:37a0b92b08d4919615c3ee023f7ddb068d12b8387475d64c622ac30f45c29c51";
    const HTPASSWD: &str = "testuser:$2y$05$8/q2bfRcX74EuxGf0qOcSuhWDQJXrgWiy6Fi73/JM2tKC66qSrLve";
    const HTPASSWD_USERNAME: &str = "testuser";
    const HTPASSWD_PASSWORD: &str = "testpassword";

    #[test]
    fn test_apply_accept() -> anyhow::Result<()> {
        assert_eq!(
            RequestBuilderWrapper::from_client(&Client::default(), |client| client
                .get("https://example.com/some/module.wasm"))
            .apply_accept(&["*/*"])?
            .into_request_builder()
            .build()?
            .headers()["Accept"],
            "*/*"
        );

        assert_eq!(
            RequestBuilderWrapper::from_client(&Client::default(), |client| client
                .get("https://example.com/some/module.wasm"))
            .apply_accept(MIME_TYPES_DISTRIBUTION_MANIFEST)?
            .into_request_builder()
            .build()?
            .headers()["Accept"],
            MIME_TYPES_DISTRIBUTION_MANIFEST.join(", ")
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_apply_auth_no_token() -> anyhow::Result<()> {
        assert!(
            !RequestBuilderWrapper::from_client(&Client::default(), |client| client
                .get("https://example.com/some/module.wasm"))
            .apply_auth(
                &Reference::try_from(HELLO_IMAGE_TAG)?,
                RegistryOperation::Pull
            )
            .await?
            .into_request_builder()
            .build()?
            .headers()
            .contains_key("Authorization")
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_apply_auth_bearer_token() -> anyhow::Result<()> {
        use hmac::{Hmac, Mac};
        use jwt::SignWithKey;
        use sha2::Sha256;
        let client = Client::default();
        let header = jwt::header::Header {
            algorithm: jwt::algorithm::AlgorithmType::Hs256,
            key_id: None,
            type_: None,
            content_type: None,
        };
        let claims: jwt::claims::Claims = Default::default();
        let key: Hmac<Sha256> = Hmac::new_from_slice(b"some-secret").unwrap();
        let token = jwt::Token::new(header, claims)
            .sign_with_key(&key)?
            .as_str()
            .to_string();

        // we have to have it in the stored auth so we'll get to the token cache check.
        client
            .store_auth(
                &Reference::try_from(HELLO_IMAGE_TAG)?.resolve_registry(),
                RegistryAuth::Anonymous,
            )
            .await;

        client
            .tokens
            .insert(
                &Reference::try_from(HELLO_IMAGE_TAG)?,
                RegistryOperation::Pull,
                RegistryTokenType::Bearer(RegistryToken::Token {
                    token: token.clone(),
                }),
            )
            .await;
        assert_eq!(
            RequestBuilderWrapper::from_client(&client, |client| client
                .get("https://example.com/some/module.wasm"))
            .apply_auth(
                &Reference::try_from(HELLO_IMAGE_TAG)?,
                RegistryOperation::Pull
            )
            .await?
            .into_request_builder()
            .build()?
            .headers()["Authorization"],
            format!("Bearer {}", &token)
        );

        Ok(())
    }

    #[test]
    fn test_to_v2_blob_url() {
        let image = Reference::try_from(HELLO_IMAGE_TAG).expect("failed to parse reference");
        let blob_url = Client::default().to_v2_blob_url(
            image.registry(),
            image.repository(),
            "sha256:deadbeef",
        );
        assert_eq!(
            blob_url,
            "https://webassembly.azurecr.io/v2/hello-wasm/blobs/sha256:deadbeef"
        )
    }

    #[test]
    fn test_to_v2_manifest() {
        let c = Client::default();

        for &(image, expected_uri) in [
            (HELLO_IMAGE_NO_TAG, "https://webassembly.azurecr.io/v2/hello-wasm/manifests/latest"), // TODO: confirm this is the right translation when no tag
            (HELLO_IMAGE_TAG, "https://webassembly.azurecr.io/v2/hello-wasm/manifests/v1"),
            (HELLO_IMAGE_DIGEST, "https://webassembly.azurecr.io/v2/hello-wasm/manifests/sha256:51d9b231d5129e3ffc267c9d455c49d789bf3167b611a07ab6e4b3304c96b0e7"),
            (HELLO_IMAGE_TAG_AND_DIGEST, "https://webassembly.azurecr.io/v2/hello-wasm/manifests/sha256:51d9b231d5129e3ffc267c9d455c49d789bf3167b611a07ab6e4b3304c96b0e7"),
            ].iter() {
                let reference = Reference::try_from(image).expect("failed to parse reference");
                assert_eq!(c.to_v2_manifest_url(&reference), expected_uri);
            }
    }

    #[test]
    fn test_to_v2_blob_upload_url() {
        let image = Reference::try_from(HELLO_IMAGE_TAG).expect("failed to parse reference");
        let blob_url = Client::default().to_v2_blob_upload_url(&image);

        assert_eq!(
            blob_url,
            "https://webassembly.azurecr.io/v2/hello-wasm/blobs/uploads/"
        )
    }

    #[test]
    fn test_to_list_tags_url() {
        let image = Reference::try_from(HELLO_IMAGE_TAG).expect("failed to parse reference");
        let blob_url = Client::default().to_list_tags_url(&image);

        assert_eq!(
            blob_url,
            "https://webassembly.azurecr.io/v2/hello-wasm/tags/list"
        )
    }

    #[test]
    fn manifest_url_generation_respects_http_protocol() {
        let c = Client::new(ClientConfig {
            protocol: ClientProtocol::Http,
            ..Default::default()
        });
        let reference = Reference::try_from("webassembly.azurecr.io/hello:v1".to_owned())
            .expect("Could not parse reference");
        assert_eq!(
            "http://webassembly.azurecr.io/v2/hello/manifests/v1",
            c.to_v2_manifest_url(&reference)
        );
    }

    #[test]
    fn blob_url_generation_respects_http_protocol() {
        let c = Client::new(ClientConfig {
            protocol: ClientProtocol::Http,
            ..Default::default()
        });
        let reference = Reference::try_from("webassembly.azurecr.io/hello@sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff".to_owned())
            .expect("Could not parse reference");
        assert_eq!(
            "http://webassembly.azurecr.io/v2/hello/blobs/sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
            c.to_v2_blob_url(
                reference.registry(),
                reference.repository(),
                reference.digest().unwrap()
            )
        );
    }

    #[test]
    fn manifest_url_generation_uses_https_if_not_on_exception_list() {
        let insecure_registries = vec!["localhost".to_owned(), "oci.registry.local".to_owned()];
        let protocol = ClientProtocol::HttpsExcept(insecure_registries);
        let c = Client::new(ClientConfig {
            protocol,
            ..Default::default()
        });
        let reference = Reference::try_from("webassembly.azurecr.io/hello:v1".to_owned())
            .expect("Could not parse reference");
        assert_eq!(
            "https://webassembly.azurecr.io/v2/hello/manifests/v1",
            c.to_v2_manifest_url(&reference)
        );
    }

    #[test]
    fn manifest_url_generation_uses_http_if_on_exception_list() {
        let insecure_registries = vec!["localhost".to_owned(), "oci.registry.local".to_owned()];
        let protocol = ClientProtocol::HttpsExcept(insecure_registries);
        let c = Client::new(ClientConfig {
            protocol,
            ..Default::default()
        });
        let reference = Reference::try_from("oci.registry.local/hello:v1".to_owned())
            .expect("Could not parse reference");
        assert_eq!(
            "http://oci.registry.local/v2/hello/manifests/v1",
            c.to_v2_manifest_url(&reference)
        );
    }

    #[test]
    fn blob_url_generation_uses_https_if_not_on_exception_list() {
        let insecure_registries = vec!["localhost".to_owned(), "oci.registry.local".to_owned()];
        let protocol = ClientProtocol::HttpsExcept(insecure_registries);
        let c = Client::new(ClientConfig {
            protocol,
            ..Default::default()
        });
        let reference = Reference::try_from("webassembly.azurecr.io/hello@sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff".to_owned())
            .expect("Could not parse reference");
        assert_eq!(
            "https://webassembly.azurecr.io/v2/hello/blobs/sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
            c.to_v2_blob_url(
                reference.registry(),
                reference.repository(),
                reference.digest().unwrap()
            )
        );
    }

    #[test]
    fn blob_url_generation_uses_http_if_on_exception_list() {
        let insecure_registries = vec!["localhost".to_owned(), "oci.registry.local".to_owned()];
        let protocol = ClientProtocol::HttpsExcept(insecure_registries);
        let c = Client::new(ClientConfig {
            protocol,
            ..Default::default()
        });
        let reference = Reference::try_from("oci.registry.local/hello@sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff".to_owned())
            .expect("Could not parse reference");
        assert_eq!(
            "http://oci.registry.local/v2/hello/blobs/sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
            c.to_v2_blob_url(
                reference.registry(),
                reference.repository(),
                reference.digest().unwrap()
            )
        );
    }

    #[test]
    fn can_generate_valid_digest() {
        let bytes = b"hellobytes";
        let hash = sha256_digest(bytes);

        let combination = vec![b"hello".to_vec(), b"bytes".to_vec()];
        let combination_hash =
            sha256_digest(&combination.into_iter().flatten().collect::<Vec<u8>>());

        assert_eq!(
            hash,
            "sha256:fdbd95aafcbc814a2600fcc54c1e1706f52d2f9bf45cf53254f25bcd7599ce99"
        );
        assert_eq!(
            combination_hash,
            "sha256:fdbd95aafcbc814a2600fcc54c1e1706f52d2f9bf45cf53254f25bcd7599ce99"
        );
    }

    #[test]
    fn test_registry_token_deserialize() {
        // 'token' field, standalone
        let text = r#"{"token": "abc"}"#;
        let res: Result<RegistryToken, serde_json::Error> = serde_json::from_str(text);
        assert!(res.is_ok());
        let rt = res.unwrap();
        assert_eq!(rt.token(), "abc");

        // 'access_token' field, standalone
        let text = r#"{"access_token": "xyz"}"#;
        let res: Result<RegistryToken, serde_json::Error> = serde_json::from_str(text);
        assert!(res.is_ok());
        let rt = res.unwrap();
        assert_eq!(rt.token(), "xyz");

        // both 'token' and 'access_token' fields, 'token' field takes precedence
        let text = r#"{"access_token": "xyz", "token": "abc"}"#;
        let res: Result<RegistryToken, serde_json::Error> = serde_json::from_str(text);
        assert!(res.is_ok());
        let rt = res.unwrap();
        assert_eq!(rt.token(), "abc");

        // both 'token' and 'access_token' fields, 'token' field takes precedence (reverse order)
        let text = r#"{"token": "abc", "access_token": "xyz"}"#;
        let res: Result<RegistryToken, serde_json::Error> = serde_json::from_str(text);
        assert!(res.is_ok());
        let rt = res.unwrap();
        assert_eq!(rt.token(), "abc");

        // non-string fields do not break parsing
        let text = r#"{"aaa": 300, "access_token": "xyz", "token": "abc", "zzz": 600}"#;
        let res: Result<RegistryToken, serde_json::Error> = serde_json::from_str(text);
        assert!(res.is_ok());

        // Note: tokens should always be strings. The next two tests ensure that if one field
        // is invalid (integer), then parse can still succeed if the other field is a string.
        //
        // numeric 'access_token' field, but string 'token' field does not in parse error
        let text = r#"{"access_token": 300, "token": "abc"}"#;
        let res: Result<RegistryToken, serde_json::Error> = serde_json::from_str(text);
        assert!(res.is_ok());
        let rt = res.unwrap();
        assert_eq!(rt.token(), "abc");

        // numeric 'token' field, but string 'accesss_token' field does not in parse error
        let text = r#"{"access_token": "xyz", "token": 300}"#;
        let res: Result<RegistryToken, serde_json::Error> = serde_json::from_str(text);
        assert!(res.is_ok());
        let rt = res.unwrap();
        assert_eq!(rt.token(), "xyz");

        // numeric 'token' field results in parse error
        let text = r#"{"token": 300}"#;
        let res: Result<RegistryToken, serde_json::Error> = serde_json::from_str(text);
        assert!(res.is_err());

        // numeric 'access_token' field results in parse error
        let text = r#"{"access_token": 300}"#;
        let res: Result<RegistryToken, serde_json::Error> = serde_json::from_str(text);
        assert!(res.is_err());

        // object 'token' field results in parse error
        let text = r#"{"token": {"some": "thing"}}"#;
        let res: Result<RegistryToken, serde_json::Error> = serde_json::from_str(text);
        assert!(res.is_err());

        // object 'access_token' field results in parse error
        let text = r#"{"access_token": {"some": "thing"}}"#;
        let res: Result<RegistryToken, serde_json::Error> = serde_json::from_str(text);
        assert!(res.is_err());

        // missing fields results in parse error
        let text = r#"{"some": "thing"}"#;
        let res: Result<RegistryToken, serde_json::Error> = serde_json::from_str(text);
        assert!(res.is_err());

        // bad JSON results in parse error
        let text = r#"{"token": "abc""#;
        let res: Result<RegistryToken, serde_json::Error> = serde_json::from_str(text);
        assert!(res.is_err());

        // worse JSON results in parse error
        let text = r#"_ _ _ kjbwef??98{9898 }} }}"#;
        let res: Result<RegistryToken, serde_json::Error> = serde_json::from_str(text);
        assert!(res.is_err());
    }

    fn check_auth_token(token: &str) {
        // We test that the token is longer than a minimal hash.
        assert!(token.len() > 64);
    }

    #[tokio::test]
    async fn test_auth() {
        for &image in TEST_IMAGES {
            let reference = Reference::try_from(image).expect("failed to parse reference");
            let c = Client::default();
            let token = c
                .auth(
                    &reference,
                    &RegistryAuth::Anonymous,
                    RegistryOperation::Pull,
                )
                .await
                .expect("result from auth request");

            assert!(token.is_some());
            check_auth_token(token.unwrap().as_ref());

            let tok = c
                .tokens
                .get(&reference, RegistryOperation::Pull)
                .await
                .expect("token is available");
            // We test that the token is longer than a minimal hash.
            if let RegistryTokenType::Bearer(tok) = tok {
                check_auth_token(tok.token());
            } else {
                panic!("Unexpeted Basic Auth Token");
            }
        }
    }

    #[cfg(feature = "test-registry")]
    #[tokio::test]
    async fn test_list_tags() {
        let docker = clients::Cli::default();
        let test_container = docker.run(registry_image_edge());
        let port = test_container.get_host_port_ipv4(5000);
        let auth =
            RegistryAuth::Basic(HTPASSWD_USERNAME.to_string(), HTPASSWD_PASSWORD.to_string());

        let client = Client::new(ClientConfig {
            protocol: ClientProtocol::HttpsExcept(vec![format!("localhost:{}", port)]),
            ..Default::default()
        });

        let image: Reference = HELLO_IMAGE_TAG_AND_DIGEST.parse().unwrap();
        client
            .auth(&image, &RegistryAuth::Anonymous, RegistryOperation::Pull)
            .await
            .expect("cannot authenticate against registry for pull operation");

        let (manifest, _digest) = client
            ._pull_image_manifest(&image)
            .await
            .expect("failed to pull manifest");

        let image_data = client
            .pull(&image, &auth, vec![manifest::WASM_LAYER_MEDIA_TYPE])
            .await
            .expect("failed to pull image");

        for i in 0..=3 {
            let push_image: Reference = format!("localhost:{}/hello-wasm:1.0.{}", port, i)
                .parse()
                .unwrap();
            client
                .auth(&push_image, &auth, RegistryOperation::Push)
                .await
                .expect("authenticated");
            client
                .push(
                    &push_image,
                    &image_data.layers,
                    image_data.config.clone(),
                    &auth,
                    Some(manifest.clone()),
                )
                .await
                .expect("Failed to push Image");
        }

        let image: Reference = format!("localhost:{}/hello-wasm:1.0.1", port)
            .parse()
            .unwrap();
        let response = client
            .list_tags(&image, &RegistryAuth::Anonymous, Some(2), Some("1.0.1"))
            .await
            .expect("Cannot list Tags");
        assert_eq!(response.tags, vec!["1.0.2", "1.0.3"])
    }

    #[tokio::test]
    async fn test_pull_manifest_private() {
        for &image in TEST_IMAGES {
            let reference = Reference::try_from(image).expect("failed to parse reference");
            // Currently, pull_manifest does not perform Authz, so this will fail.
            let c = Client::default();
            c._pull_image_manifest(&reference)
                .await
                .expect_err("pull manifest should fail");

            // But this should pass
            let c = Client::default();
            c.auth(
                &reference,
                &RegistryAuth::Anonymous,
                RegistryOperation::Pull,
            )
            .await
            .expect("authenticated");
            let (manifest, _) = c
                ._pull_image_manifest(&reference)
                .await
                .expect("pull manifest should not fail");

            // The test on the manifest checks all fields. This is just a brief sanity check.
            assert_eq!(manifest.schema_version, 2);
            assert!(!manifest.layers.is_empty());
        }
    }

    #[tokio::test]
    async fn test_pull_manifest_public() {
        for &image in TEST_IMAGES {
            let reference = Reference::try_from(image).expect("failed to parse reference");
            let c = Client::default();
            let (manifest, _) = c
                .pull_image_manifest(&reference, &RegistryAuth::Anonymous)
                .await
                .expect("pull manifest should not fail");

            // The test on the manifest checks all fields. This is just a brief sanity check.
            assert_eq!(manifest.schema_version, 2);
            assert!(!manifest.layers.is_empty());
        }
    }

    #[tokio::test]
    async fn pull_manifest_and_config_public() {
        for &image in TEST_IMAGES {
            let reference = Reference::try_from(image).expect("failed to parse reference");
            let c = Client::default();
            let (manifest, _, config) = c
                .pull_manifest_and_config(&reference, &RegistryAuth::Anonymous)
                .await
                .expect("pull manifest and config should not fail");

            // The test on the manifest checks all fields. This is just a brief sanity check.
            assert_eq!(manifest.schema_version, 2);
            assert!(!manifest.layers.is_empty());
            assert!(!config.is_empty());
        }
    }

    #[tokio::test]
    async fn test_fetch_digest() {
        let c = Client::default();

        for &image in TEST_IMAGES {
            let reference = Reference::try_from(image).expect("failed to parse reference");
            c.fetch_manifest_digest(&reference, &RegistryAuth::Anonymous)
                .await
                .expect("pull manifest should not fail");

            // This should pass
            let reference = Reference::try_from(image).expect("failed to parse reference");
            let c = Client::default();
            c.auth(
                &reference,
                &RegistryAuth::Anonymous,
                RegistryOperation::Pull,
            )
            .await
            .expect("authenticated");
            let digest = c
                .fetch_manifest_digest(&reference, &RegistryAuth::Anonymous)
                .await
                .expect("pull manifest should not fail");

            assert_eq!(
                digest,
                "sha256:51d9b231d5129e3ffc267c9d455c49d789bf3167b611a07ab6e4b3304c96b0e7"
            );
        }
    }

    #[tokio::test]
    async fn test_pull_blob() {
        let c = Client::default();

        for &image in TEST_IMAGES {
            let reference = Reference::try_from(image).expect("failed to parse reference");
            c.auth(
                &reference,
                &RegistryAuth::Anonymous,
                RegistryOperation::Pull,
            )
            .await
            .expect("authenticated");
            let (manifest, _) = c
                ._pull_image_manifest(&reference)
                .await
                .expect("failed to pull manifest");

            // Pull one specific layer
            let mut file: Vec<u8> = Vec::new();
            let layer0 = &manifest.layers[0];

            // This call likes to flake, so we try it at least 5 times
            let mut last_error = None;
            for i in 1..6 {
                if let Err(e) = c.pull_blob(&reference, &layer0, &mut file).await {
                    println!(
                        "Got error on pull_blob call attempt {}. Will retry in 1s: {:?}",
                        i, e
                    );
                    last_error.replace(e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                } else {
                    last_error = None;
                    break;
                }
            }

            if let Some(e) = last_error {
                panic!("Unable to pull layer: {:?}", e);
            }

            // The manifest says how many bytes we should expect.
            assert_eq!(file.len(), layer0.size as usize);
        }
    }

    #[tokio::test]
    async fn test_pull_blob_stream() {
        let c = Client::default();

        for &image in TEST_IMAGES {
            let reference = Reference::try_from(image).expect("failed to parse reference");
            c.auth(
                &reference,
                &RegistryAuth::Anonymous,
                RegistryOperation::Pull,
            )
            .await
            .expect("authenticated");
            let (manifest, _) = c
                ._pull_image_manifest(&reference)
                .await
                .expect("failed to pull manifest");

            // Pull one specific layer
            let mut file: Vec<u8> = Vec::new();
            let layer0 = &manifest.layers[0];

            let layer_stream = c
                .pull_blob_stream(&reference, &layer0)
                .await
                .expect("failed to pull blob stream");

            AsyncReadExt::read_to_end(&mut StreamReader::new(layer_stream), &mut file)
                .await
                .unwrap();

            // The manifest says how many bytes we should expect.
            assert_eq!(file.len(), layer0.size as usize);
        }
    }

    #[tokio::test]
    async fn test_pull() {
        for &image in TEST_IMAGES {
            let reference = Reference::try_from(image).expect("failed to parse reference");

            // This call likes to flake, so we try it at least 5 times
            let mut last_error = None;
            let mut image_data = None;
            for i in 1..6 {
                match Client::default()
                    .pull(
                        &reference,
                        &RegistryAuth::Anonymous,
                        vec![manifest::WASM_LAYER_MEDIA_TYPE],
                    )
                    .await
                {
                    Ok(data) => {
                        image_data = Some(data);
                        last_error = None;
                        break;
                    }
                    Err(e) => {
                        println!(
                            "Got error on pull call attempt {}. Will retry in 1s: {:?}",
                            i, e
                        );
                        last_error.replace(e);
                        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    }
                }
            }

            if let Some(e) = last_error {
                panic!("Unable to pull layer: {:?}", e);
            }

            assert!(image_data.is_some());
            let image_data = image_data.unwrap();
            assert!(!image_data.layers.is_empty());
            assert!(image_data.digest.is_some());
        }
    }

    /// Attempting to pull an image without any layer validation should fail.
    #[tokio::test]
    async fn test_pull_without_layer_validation() {
        for &image in TEST_IMAGES {
            let reference = Reference::try_from(image).expect("failed to parse reference");
            assert!(Client::default()
                .pull(&reference, &RegistryAuth::Anonymous, vec![],)
                .await
                .is_err());
        }
    }

    /// Attempting to pull an image with the wrong list of layer validations should fail.
    #[tokio::test]
    async fn test_pull_wrong_layer_validation() {
        for &image in TEST_IMAGES {
            let reference = Reference::try_from(image).expect("failed to parse reference");
            assert!(Client::default()
                .pull(&reference, &RegistryAuth::Anonymous, vec!["text/plain"],)
                .await
                .is_err());
        }
    }

    // This is the latest build of distribution/distribution from the `main` branch
    // Until distribution v3 is relased, this is the only way to have this fix
    // https://github.com/distribution/distribution/pull/3143
    //
    // We require this fix only when testing the capability to list tags
    #[cfg(feature = "test-registry")]
    fn registry_image_edge() -> GenericImage {
        GenericImage::new("distribution/distribution", "edge")
            .with_wait_for(WaitFor::message_on_stderr("listening on "))
    }

    #[cfg(feature = "test-registry")]
    fn registry_image() -> GenericImage {
        GenericImage::new("docker.io/library/registry", "2")
            .with_wait_for(WaitFor::message_on_stderr("listening on "))
    }

    #[cfg(feature = "test-registry")]
    fn registry_image_basic_auth(auth_path: &str) -> GenericImage {
        GenericImage::new("docker.io/library/registry", "2")
            .with_env_var("REGISTRY_AUTH", "htpasswd")
            .with_env_var("REGISTRY_AUTH_HTPASSWD_REALM", "Registry Realm")
            .with_env_var("REGISTRY_AUTH_HTPASSWD_PATH", "/auth/htpasswd")
            .with_volume(auth_path, "/auth")
            .with_wait_for(WaitFor::message_on_stderr("listening on "))
    }

    #[tokio::test]
    #[cfg(feature = "test-registry")]
    async fn can_push_chunk() {
        let docker = clients::Cli::default();
        let test_container = docker.run(registry_image());
        let port = test_container.get_host_port_ipv4(5000);

        let c = Client::new(ClientConfig {
            protocol: ClientProtocol::Http,
            ..Default::default()
        });
        let url = format!("localhost:{}/hello-wasm:v1", port);
        let image: Reference = url.parse().unwrap();

        c.auth(&image, &RegistryAuth::Anonymous, RegistryOperation::Push)
            .await
            .expect("result from auth request");

        let location = c
            .begin_push_chunked_session(&image)
            .await
            .expect("failed to begin push session");

        let image_data: Vec<Vec<u8>> = vec![b"iamawebassemblymodule".to_vec()];

        let (next_location, next_byte) = c
            .push_chunk(&location, &image, &image_data[0], 0)
            .await
            .expect("failed to push layer");

        // Location should include original URL with at session ID appended
        assert!(next_location.len() >= url.len() + "6987887f-0196-45ee-91a1-2dfad901bea0".len());
        assert_eq!(
            next_byte,
            "iamawebassemblymodule".to_string().into_bytes().len()
        );

        let layer_location = c
            .end_push_chunked_session(&next_location, &image, &sha256_digest(&image_data[0]))
            .await
            .expect("failed to end push session");

        assert_eq!(layer_location, format!("http://localhost:{}/v2/hello-wasm/blobs/sha256:6165c4ad43c0803798b6f2e49d6348c915d52c999a5f890846cee77ea65d230b", port));
    }

    #[tokio::test]
    #[cfg(feature = "test-registry")]
    async fn can_push_multiple_chunks() {
        let docker = clients::Cli::default();
        let test_container = docker.run(registry_image());
        let port = test_container.get_host_port_ipv4(5000);

        let mut c = Client::new(ClientConfig {
            protocol: ClientProtocol::Http,
            ..Default::default()
        });
        // set a super small chunk size - done to force multiple pushes
        c.push_chunk_size = 3;
        let url = format!("localhost:{}/hello-wasm:v1", port);
        let image: Reference = url.parse().unwrap();

        c.auth(&image, &RegistryAuth::Anonymous, RegistryOperation::Push)
            .await
            .expect("result from auth request");

        let image_data: Vec<u8> =
            b"i am a big webassembly mode that needs chunked uploads".to_vec();
        let image_digest = sha256_digest(&image_data);

        let location = c
            .push_blob_chunked(&image, &image_data, &image_digest)
            .await
            .expect("failed to begin push session");

        assert_eq!(
            location,
            format!(
                "http://localhost:{}/v2/hello-wasm/blobs/{}",
                port, image_digest
            )
        );
    }

    #[tokio::test]
    #[cfg(feature = "test-registry")]
    async fn test_image_roundtrip_anon_auth() {
        let docker = clients::Cli::default();
        let test_container = docker.run(registry_image());

        test_image_roundtrip(&RegistryAuth::Anonymous, &test_container).await;
    }

    #[tokio::test]
    #[cfg(feature = "test-registry")]
    async fn test_image_roundtrip_basic_auth() {
        let auth_dir = TempDir::new().expect("cannot create tmp directory");
        let htpasswd_path = path::Path::join(auth_dir.path(), "htpasswd");
        fs::write(htpasswd_path, HTPASSWD).expect("cannot write htpasswd file");

        let docker = clients::Cli::default();
        let image = registry_image_basic_auth(
            auth_dir
                .path()
                .to_str()
                .expect("cannot convert htpasswd_path to string"),
        );
        let test_container = docker.run(image);

        let auth =
            RegistryAuth::Basic(HTPASSWD_USERNAME.to_string(), HTPASSWD_PASSWORD.to_string());

        test_image_roundtrip(&auth, &test_container).await;
    }

    #[cfg(feature = "test-registry")]
    async fn test_image_roundtrip(
        registry_auth: &RegistryAuth,
        test_container: &testcontainers::Container<'_, GenericImage>,
    ) {
        let _ = tracing_subscriber::fmt::try_init();
        let port = test_container.get_host_port_ipv4(5000);

        let c = Client::new(ClientConfig {
            protocol: ClientProtocol::HttpsExcept(vec![format!("localhost:{}", port)]),
            ..Default::default()
        });

        // pulling webassembly.azurecr.io/hello-wasm:v1
        let image: Reference = HELLO_IMAGE_TAG_AND_DIGEST.parse().unwrap();
        c.auth(&image, &RegistryAuth::Anonymous, RegistryOperation::Pull)
            .await
            .expect("cannot authenticate against registry for pull operation");

        let (manifest, _digest) = c
            ._pull_image_manifest(&image)
            .await
            .expect("failed to pull manifest");

        let image_data = c
            .pull(&image, registry_auth, vec![manifest::WASM_LAYER_MEDIA_TYPE])
            .await
            .expect("failed to pull image");

        let push_image: Reference = format!("localhost:{}/hello-wasm:v1", port).parse().unwrap();
        c.auth(&push_image, registry_auth, RegistryOperation::Push)
            .await
            .expect("authenticated");

        c.push(
            &push_image,
            &image_data.layers,
            image_data.config.clone(),
            registry_auth,
            Some(manifest.clone()),
        )
        .await
        .expect("failed to push image");

        let pulled_image_data = c
            .pull(
                &push_image,
                registry_auth,
                vec![manifest::WASM_LAYER_MEDIA_TYPE],
            )
            .await
            .expect("failed to pull pushed image");

        let (pulled_manifest, _digest) = c
            ._pull_image_manifest(&push_image)
            .await
            .expect("failed to pull pushed image manifest");

        assert!(image_data.layers.len() == 1);
        assert!(pulled_image_data.layers.len() == 1);
        assert_eq!(
            image_data.layers[0].data.len(),
            pulled_image_data.layers[0].data.len()
        );
        assert_eq!(image_data.layers[0].data, pulled_image_data.layers[0].data);

        assert_eq!(manifest.media_type, pulled_manifest.media_type);
        assert_eq!(manifest.schema_version, pulled_manifest.schema_version);
        assert_eq!(manifest.config.digest, pulled_manifest.config.digest);
    }

    #[tokio::test]
    async fn test_raw_manifest_digest() {
        let _ = tracing_subscriber::fmt::try_init();

        let c = Client::default();

        // pulling webassembly.azurecr.io/hello-wasm:v1@sha256:51d9b231d5129e3ffc267c9d455c49d789bf3167b611a07ab6e4b3304c96b0e7
        let image: Reference = HELLO_IMAGE_TAG_AND_DIGEST.parse().unwrap();
        c.auth(&image, &RegistryAuth::Anonymous, RegistryOperation::Pull)
            .await
            .expect("cannot authenticate against registry for pull operation");

        let (manifest, _) = c
            .pull_manifest_raw(
                &image,
                &RegistryAuth::Anonymous,
                MIME_TYPES_DISTRIBUTION_MANIFEST,
            )
            .await
            .expect("failed to pull manifest");

        // Compute the digest of the returned manifest text.
        let digest = sha2::Sha256::digest(manifest);
        let hex = format!("sha256:{:x}", digest);

        // Validate that the computed digest and the digest in the pulled reference match.
        assert_eq!(image.digest().unwrap(), hex);
    }

    #[tokio::test]
    #[cfg(feature = "test-registry")]
    async fn test_mount() {
        // initialize the registry
        let docker = clients::Cli::default();
        let test_container = docker.run(registry_image());
        let port = test_container.get_host_port_ipv4(5000);

        let c = Client::new(ClientConfig {
            protocol: ClientProtocol::HttpsExcept(vec![format!("localhost:{}", port)]),
            ..Default::default()
        });

        // Create a dummy layer and push it to `layer-repository`
        let layer_reference: Reference = format!("localhost:{}/layer-repository", port)
            .parse()
            .unwrap();
        let layer_data = vec![1u8, 2, 3, 4];
        let layer = OciDescriptor {
            digest: sha256_digest(&layer_data),
            ..Default::default()
        };
        c.push_blob(&layer_reference, &[1, 2, 3, 4], &layer.digest)
            .await
            .expect("Failed to push");

        // Mount the layer at `image-repository`
        let image_reference: Reference = format!("localhost:{}/image-repository", port)
            .parse()
            .unwrap();
        c.mount_blob(&image_reference, &layer_reference, &layer.digest)
            .await
            .expect("Failed to mount");

        // Pull the layer from `image-repository`
        let mut buf = Vec::new();
        c.pull_blob(&image_reference, &layer, &mut buf)
            .await
            .expect("Failed to pull");

        assert_eq!(layer_data, buf);
    }

    #[tokio::test]
    async fn test_platform_resolution() {
        // test that we get an error when we pull a manifest list
        let reference = Reference::try_from(DOCKER_IO_IMAGE).expect("failed to parse reference");
        let mut c = Client::new(ClientConfig {
            platform_resolver: None,
            ..Default::default()
        });
        let err = c
            .pull_image_manifest(&reference, &RegistryAuth::Anonymous)
            .await
            .unwrap_err();
        assert_eq!(
            format!("{}", err),
            "Received Image Index/Manifest List, but platform_resolver was not defined on the client config. Consider setting platform_resolver"
        );

        c = Client::new(ClientConfig {
            platform_resolver: Some(Box::new(linux_amd64_resolver)),
            ..Default::default()
        });
        let (_manifest, digest) = c
            .pull_image_manifest(&reference, &RegistryAuth::Anonymous)
            .await
            .expect("Couldn't pull manifest");
        assert_eq!(
            digest,
            "sha256:f54a58bc1aac5ea1a25d796ae155dc228b3f0e11d046ae276b39c4bf2f13d8c4"
        );
    }

    #[tokio::test]
    async fn test_pull_ghcr_io() {
        let reference = Reference::try_from(GHCR_IO_IMAGE).expect("failed to parse reference");
        let c = Client::default();
        let (manifest, _manifest_str) = c
            .pull_image_manifest(&reference, &RegistryAuth::Anonymous)
            .await
            .unwrap();
        assert_eq!(manifest.config.media_type, manifest::WASM_CONFIG_MEDIA_TYPE);
    }

    #[tokio::test]
    #[ignore]
    async fn test_roundtrip_multiple_layers() {
        let _ = tracing_subscriber::fmt::try_init();
        let c = Client::new(ClientConfig {
            protocol: ClientProtocol::HttpsExcept(vec!["oci.registry.local".to_string()]),
            ..Default::default()
        });
        let src_image = Reference::try_from("registry:2.7.1").expect("failed to parse reference");
        let dest_image = Reference::try_from("oci.registry.local/registry:roundtrip-test")
            .expect("failed to parse reference");

        let image = c
            .pull(
                &src_image,
                &RegistryAuth::Anonymous,
                vec![IMAGE_DOCKER_LAYER_GZIP_MEDIA_TYPE],
            )
            .await
            .expect("Failed to pull manifest");
        assert!(image.layers.len() > 1);

        let ImageData {
            layers,
            config,
            manifest,
            ..
        } = image;
        c.push(
            &dest_image,
            &layers,
            config,
            &RegistryAuth::Anonymous,
            manifest,
        )
        .await
        .expect("Failed to pull manifest");

        c.pull_image_manifest(&dest_image, &RegistryAuth::Anonymous)
            .await
            .expect("Failed to pull manifest");
    }
}
