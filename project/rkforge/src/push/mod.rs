mod pusher;

use crate::config::auth::AuthConfig;
use crate::push::pusher::{PushTask, Pusher};
use crate::registry::{
    RegistryScheme, effective_skip_tls_verify, parse_registry_host, parse_registry_host_arg,
    resolve_client_ref_auth as resolve_ref_with_auth,
};
use crate::rt::block_on;
use crate::storage::{DigestExt, parse_image_ref};
use anyhow::{Context, bail};
use clap::Parser;
use futures::{StreamExt, TryStreamExt, stream};
use oci_client::Client;
use oci_client::client::PushResponse;
use oci_client::manifest::{OciImageIndex, OciManifest};
use oci_client::secrets::RegistryAuth;
use oci_spec::distribution::Reference;
use reqwest::{StatusCode, Url};
use std::collections::HashMap;
use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

#[derive(Parser, Debug)]
pub struct PushArgs {
    /// Image reference
    image_ref: String,
    /// Image path (default current directory)
    #[arg(long)]
    path: Option<String>,
    /// Registry host in `host[:port]` format.
    #[arg(long, value_parser = parse_registry_host_arg)]
    url: Option<String>,
    /// Skip TLS certificate verification for HTTPS registry.
    #[arg(long)]
    skip_tls_verify: bool,
}

pub fn push(args: PushArgs) -> anyhow::Result<()> {
    let path = args.path.unwrap_or(".".to_string());
    push_from_layout_with_tls(args.image_ref, path, args.url, args.skip_tls_verify)
}

pub fn push_from_layout(
    image_ref: impl Into<String>,
    path: impl AsRef<Path>,
    url: Option<String>,
) -> anyhow::Result<()> {
    push_from_layout_with_tls(image_ref, path, url, false)
}

fn push_from_layout_with_tls(
    image_ref: impl Into<String>,
    path: impl AsRef<Path>,
    url: Option<String>,
    skip_tls_verify: bool,
) -> anyhow::Result<()> {
    let image_ref = image_ref.into();
    let path = path.as_ref().to_path_buf();
    let auth_config = AuthConfig::load()?;
    let requested_has_explicit_tag = has_explicit_tag(&image_ref);
    let parsed_input_ref = image_ref
        .parse::<Reference>()
        .with_context(|| format!("invalid image reference for push: {}", image_ref.as_str()))?;
    let requested_repo = parsed_input_ref.repository().to_string();

    let url = match url {
        Some(url) => auth_config.resolve_url(Some(url))?,
        None if has_explicit_registry(&image_ref) => {
            parse_registry_host(parsed_input_ref.registry())?
        }
        None => auth_config.resolve_url(None::<String>)?,
    };

    let normalized_image_ref = if let Some(tag) = parsed_input_ref.tag() {
        format!("{requested_repo}:{}", tag)
    } else {
        requested_repo.clone()
    };
    let registry_scheme = auth_config.registry_scheme(&url);
    let (client, image_ref, auth_method) =
        resolve_ref_with_auth(&auth_config, &url, &normalized_image_ref, skip_tls_verify)?;
    let registry_url = image_ref.registry().to_string();
    let blob_uploader = BlobUploader::new(
        registry_scheme,
        &registry_url,
        auth_method.clone(),
        skip_tls_verify,
    )?;

    block_on(async move {
        push_image(PushImageContext {
            client: &client,
            blob_uploader: &blob_uploader,
            image_ref: &image_ref,
            auth_method: &auth_method,
            path: &path,
            registry_url: &registry_url,
            requested_repo: &requested_repo,
            requested_has_explicit_tag,
        })
        .await
    })?
}

#[derive(Clone)]
struct BlobUploader {
    http: reqwest::Client,
    scheme: RegistryScheme,
    registry: String,
    auth: RegistryAuth,
}

struct PushImageContext<'a> {
    client: &'a Client,
    blob_uploader: &'a BlobUploader,
    image_ref: &'a Reference,
    auth_method: &'a RegistryAuth,
    path: &'a Path,
    registry_url: &'a str,
    requested_repo: &'a str,
    requested_has_explicit_tag: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BlobUploadStrategy {
    Monolithic,
    Chunked,
}

const SMALL_BLOB_UPLOAD_THRESHOLD_BYTES: u64 = 16 * 1024 * 1024;
const CHUNKED_BLOB_UPLOAD_SIZE_BYTES: usize = 32 * 1024 * 1024;
const MAX_CONCURRENT_BLOB_UPLOADS: usize = 4;
const STREAM_READ_BUFFER_BYTES: usize = 1024 * 1024;

fn blob_upload_strategy(size: u64) -> BlobUploadStrategy {
    if size <= SMALL_BLOB_UPLOAD_THRESHOLD_BYTES {
        BlobUploadStrategy::Monolithic
    } else {
        BlobUploadStrategy::Chunked
    }
}

impl BlobUploader {
    fn new(
        scheme: RegistryScheme,
        registry: impl Into<String>,
        auth: RegistryAuth,
        skip_tls_verify: bool,
    ) -> anyhow::Result<Self> {
        let registry = registry.into();
        let http = reqwest::Client::builder()
            .danger_accept_invalid_certs(effective_skip_tls_verify(
                skip_tls_verify,
                scheme,
                &registry,
            ))
            .build()
            .context("failed to create blob upload HTTP client")?;
        Ok(Self {
            http,
            scheme,
            registry,
            auth,
        })
    }

    fn apply_auth(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.auth {
            RegistryAuth::Anonymous => builder,
            RegistryAuth::Basic(username, password) => builder.basic_auth(username, Some(password)),
            RegistryAuth::Bearer(token) => builder.bearer_auth(token),
        }
    }

    fn uploads_url(&self, target_ref: &Reference) -> String {
        format!(
            "{}://{}/v2/{}/blobs/uploads/",
            self.scheme.as_str(),
            self.registry,
            target_ref.repository()
        )
    }

    fn blob_url(&self, target_ref: &Reference, digest: &str) -> String {
        format!(
            "{}://{}/v2/{}/blobs/{}",
            self.scheme.as_str(),
            self.registry,
            target_ref.repository(),
            digest
        )
    }

    fn location_header_to_url(&self, location: &str) -> anyhow::Result<Url> {
        if location.starts_with('/') {
            Ok(Url::parse(&format!(
                "{}://{}{}",
                self.scheme.as_str(),
                self.registry,
                location
            ))?)
        } else {
            Ok(Url::parse(location)?)
        }
    }

    async fn begin_upload(&self, target_ref: &Reference) -> anyhow::Result<Url> {
        let url = self.uploads_url(target_ref);
        let response = self
            .apply_auth(self.http.post(&url))
            .header("Content-Length", 0)
            .send()
            .await
            .with_context(|| {
                format!("failed to begin upload session for {}", target_ref.whole())
            })?;

        self.extract_location(response, StatusCode::ACCEPTED)
            .await
            .with_context(|| format!("failed to open upload session for {}", target_ref.whole()))
    }

    async fn blob_exists(&self, target_ref: &Reference, digest: &str) -> anyhow::Result<bool> {
        let response = self
            .apply_auth(self.http.head(self.blob_url(target_ref, digest)))
            .send()
            .await
            .with_context(|| format!("failed to check remote blob {digest}"))?;

        match response.status() {
            StatusCode::OK => Ok(true),
            StatusCode::NOT_FOUND => Ok(false),
            status => {
                let url = response.url().to_string();
                let body = response.text().await.unwrap_or_default();
                bail!("registry returned {status} for {url}: {body}");
            }
        }
    }

    async fn extract_location(
        &self,
        response: reqwest::Response,
        expected_status: StatusCode,
    ) -> anyhow::Result<Url> {
        if response.status() != expected_status {
            let status = response.status();
            let url = response.url().to_string();
            let body = response.text().await.unwrap_or_default();
            bail!("registry returned {status} for {url}: {body}");
        }

        let location = response
            .headers()
            .get("Location")
            .context("registry response missing Location header")?
            .to_str()
            .context("registry Location header is not valid UTF-8")?;
        self.location_header_to_url(location)
    }

    async fn push_blob_from_path(
        &self,
        target_ref: &Reference,
        blob_path: &Path,
        digest: &str,
    ) -> anyhow::Result<String> {
        if self.blob_exists(target_ref, digest).await? {
            return Ok(self.blob_url(target_ref, digest));
        }

        let size = tokio::fs::metadata(blob_path)
            .await
            .with_context(|| format!("failed to stat {}", blob_path.display()))?
            .len();

        match blob_upload_strategy(size) {
            BlobUploadStrategy::Monolithic => {
                self.push_blob_monolithically_from_path(target_ref, blob_path, digest, size)
                    .await
            }
            BlobUploadStrategy::Chunked => {
                self.push_blob_chunked_from_path(target_ref, blob_path, digest, size)
                    .await
            }
        }
    }

    async fn push_blob_monolithically_from_path(
        &self,
        target_ref: &Reference,
        blob_path: &Path,
        digest: &str,
        size: u64,
    ) -> anyhow::Result<String> {
        let mut location = self.begin_upload(target_ref).await?;
        location.query_pairs_mut().append_pair("digest", digest);

        let file = tokio::fs::File::open(blob_path)
            .await
            .with_context(|| format!("failed to open {}", blob_path.display()))?;
        let body =
            reqwest::Body::wrap_stream(ReaderStream::with_capacity(file, STREAM_READ_BUFFER_BYTES));

        let response = self
            .apply_auth(self.http.put(location.clone()))
            .header("Content-Length", size)
            .header("Content-Type", "application/octet-stream")
            .body(body)
            .send()
            .await
            .with_context(|| format!("failed to upload {}", blob_path.display()))?;

        Ok(self
            .extract_location(response, StatusCode::CREATED)
            .await?
            .to_string())
    }

    async fn push_blob_chunked_from_path(
        &self,
        target_ref: &Reference,
        blob_path: &Path,
        digest: &str,
        size: u64,
    ) -> anyhow::Result<String> {
        let mut location = self.begin_upload(target_ref).await?;

        let mut start: u64 = 0;
        while start < size {
            let chunk_size = (size - start).min(CHUNKED_BLOB_UPLOAD_SIZE_BYTES as u64);
            let end = start + chunk_size - 1;
            let mut chunk_file = tokio::fs::File::open(blob_path)
                .await
                .with_context(|| format!("failed to open {}", blob_path.display()))?;
            chunk_file
                .seek(SeekFrom::Start(start))
                .await
                .with_context(|| {
                    format!(
                        "failed to seek {} to chunk offset {start}",
                        blob_path.display()
                    )
                })?;
            let body = reqwest::Body::wrap_stream(ReaderStream::with_capacity(
                chunk_file.take(chunk_size),
                STREAM_READ_BUFFER_BYTES,
            ));

            let response = self
                .apply_auth(self.http.patch(location.clone()))
                .header("Content-Range", format!("{start}-{end}"))
                .header("Content-Length", chunk_size)
                .header("Content-Type", "application/octet-stream")
                .body(body)
                .send()
                .await
                .with_context(|| format!("failed to upload chunk for {}", blob_path.display()))?;

            location = self
                .extract_location(response, StatusCode::ACCEPTED)
                .await
                .with_context(|| format!("failed to update upload session for {digest}"))?;
            start = end + 1;
        }

        let mut finalize_url = location;
        finalize_url.query_pairs_mut().append_pair("digest", digest);
        let response = self
            .apply_auth(self.http.put(finalize_url))
            .header("Content-Length", 0)
            .send()
            .await
            .with_context(|| format!("failed to finalize chunked upload for {digest}"))?;

        Ok(self
            .extract_location(response, StatusCode::CREATED)
            .await?
            .to_string())
    }
}

async fn push_layer_descriptor(
    blob_uploader: BlobUploader,
    target_ref: Reference,
    blobs_dir: PathBuf,
    descriptor: oci_client::manifest::OciDescriptor,
) -> anyhow::Result<()> {
    let layer_path = blobs_dir.join(descriptor.digest.split_digest()?);
    blob_uploader
        .push_blob_from_path(&target_ref, &layer_path, &descriptor.digest)
        .await
        .with_context(|| format!("failed to push layer {}", descriptor.digest))?;
    Ok(())
}

async fn push_target_ref(
    client: &Client,
    blob_uploader: &BlobUploader,
    target_ref: &Reference,
    auth_method: &RegistryAuth,
    blobs_dir: &Path,
    manifest: &oci_client::manifest::OciImageManifest,
) -> anyhow::Result<PushResponse> {
    client
        .store_auth_if_needed(target_ref.resolve_registry(), auth_method)
        .await;

    stream::iter(manifest.layers.iter().cloned())
        .map(|descriptor| {
            let blob_uploader = blob_uploader.clone();
            let target_ref = target_ref.clone();
            let blobs_dir = blobs_dir.to_path_buf();
            async move { push_layer_descriptor(blob_uploader, target_ref, blobs_dir, descriptor).await }
        })
        .buffer_unordered(MAX_CONCURRENT_BLOB_UPLOADS)
        .try_collect::<Vec<_>>()
        .await?;

    let config_path = blobs_dir.join(manifest.config.digest.split_digest()?);
    let config_url = blob_uploader
        .push_blob_from_path(target_ref, &config_path, &manifest.config.digest)
        .await
        .with_context(|| format!("failed to push config {}", manifest.config.digest))?;

    Ok(PushResponse {
        config_url,
        manifest_url: client
            .push_manifest(target_ref, &OciManifest::Image(manifest.clone()))
            .await
            .with_context(|| format!("failed to push manifest for {}", target_ref.whole()))?,
    })
}

async fn push_image(ctx: PushImageContext<'_>) -> anyhow::Result<()> {
    let PushImageContext {
        client,
        blob_uploader,
        image_ref,
        auth_method,
        path,
        registry_url,
        requested_repo,
        requested_has_explicit_tag,
    } = ctx;
    let dir = path;

    let image_index_path = dir.join("index.json");
    let image_index = serde_json::from_str::<OciImageIndex>(
        &tokio::fs::read_to_string(&image_index_path)
            .await
            .with_context(|| format!("Failed to read from {}", image_index_path.display()))?,
    )?;

    let dir = dir.join("blobs/sha256");
    let requested_tag = if requested_has_explicit_tag {
        image_ref.tag().map(|tag| tag.to_string())
    } else {
        None
    };

    let mut digest_to_ref_names: HashMap<String, Vec<String>> = HashMap::new();
    for descriptor in &image_index.manifests {
        let digest = descriptor.digest.split_digest()?.to_string();
        let ref_name = descriptor
            .annotations
            .as_ref()
            .and_then(|ann| ann.get("org.opencontainers.image.ref.name"))
            .cloned()
            .unwrap_or_else(|| "latest".to_string());

        let entry = digest_to_ref_names.entry(digest).or_default();
        if !entry.contains(&ref_name) {
            entry.push(ref_name);
        }
    }

    let mut tasks = Vec::new();
    let mut matched_requested_tag = false;
    for (digest, ref_names) in digest_to_ref_names {
        if !should_include_digest(requested_tag.as_deref(), &ref_names) {
            continue;
        }
        if requested_tag.is_some() {
            matched_requested_tag = true;
        }

        let manifest_path = dir.join(&digest);
        let manifest = serde_json::from_str::<OciManifest>(
            &tokio::fs::read_to_string(&manifest_path)
                .await
                .with_context(|| format!("Failed to read from {}", manifest_path.display()))?,
        )?;
        let manifest = match manifest {
            OciManifest::Image(manifest) => manifest,
            OciManifest::ImageIndex(_) => anyhow::bail!("Image indexes are not supported yet"),
        };

        let target_refs = if requested_tag.is_some() {
            vec![image_ref.clone()]
        } else {
            ref_names
                .into_iter()
                .map(|ref_name| {
                    parse_image_ref(registry_url, requested_repo, Some(ref_name.as_str()))
                })
                .collect::<anyhow::Result<Vec<_>>>()?
        };

        for target_ref in target_refs {
            let auth_method = auth_method.clone();
            let blob_uploader = blob_uploader.clone();
            let client = client.clone();
            let digest_with_ref = format!("{digest}@{}", target_ref.whole());
            let blobs_dir = dir.clone();
            let manifest = manifest.clone();
            let task = PushTask::new(
                digest_with_ref,
                Box::pin(async move {
                    push_target_ref(
                        &client,
                        &blob_uploader,
                        &target_ref,
                        &auth_method,
                        &blobs_dir,
                        &manifest,
                    )
                    .await
                }),
            );
            tasks.push(task);
        }
    }

    if let Some(tag) = requested_tag
        && !matched_requested_tag
    {
        bail!("tag `{tag}` not found in index.json");
    }

    let pusher = Pusher::new(tasks);
    pusher.push_all().await?;
    Ok(())
}

fn should_include_digest(requested_tag: Option<&str>, ref_names: &[String]) -> bool {
    match requested_tag {
        Some(tag) => ref_names.iter().any(|ref_name| ref_name == tag),
        None => true,
    }
}

fn has_explicit_tag(raw: &str) -> bool {
    let raw_without_digest = raw.split_once('@').map(|(name, _)| name).unwrap_or(raw);
    let last_colon = raw_without_digest.rfind(':');
    let last_slash = raw_without_digest.rfind('/');
    match (last_colon, last_slash) {
        (Some(colon), Some(slash)) => colon > slash,
        (Some(_), None) => true,
        _ => false,
    }
}

fn has_explicit_registry(raw: &str) -> bool {
    let raw_without_digest = raw.split_once('@').map(|(name, _)| name).unwrap_or(raw);
    let raw_without_tag = if has_explicit_tag(raw_without_digest) {
        match raw_without_digest.rfind(':') {
            Some(idx) => &raw_without_digest[..idx],
            None => raw_without_digest,
        }
    } else {
        raw_without_digest
    };

    let Some((first, _rest)) = raw_without_tag.split_once('/') else {
        return false;
    };

    first == "localhost" || first.contains('.') || first.contains(':')
}

fn strip_explicit_tag(raw: &str) -> &str {
    let raw_without_digest = raw.split_once('@').map(|(name, _)| name).unwrap_or(raw);
    if has_explicit_tag(raw)
        && let Some(idx) = raw_without_digest.rfind(':')
    {
        return &raw_without_digest[..idx];
    }
    raw_without_digest
}

#[cfg(test)]
mod tests {
    use crate::storage::parse_image_ref;

    use super::{
        BlobUploadStrategy, blob_upload_strategy, has_explicit_registry, has_explicit_tag,
        should_include_digest, strip_explicit_tag,
    };

    #[test]
    fn test_should_include_digest_for_explicit_tag() {
        let refs = vec!["latest".to_string(), "v1".to_string()];
        assert!(should_include_digest(Some("v1"), &refs));
        assert!(!should_include_digest(Some("v2"), &refs));
    }

    #[test]
    fn test_should_include_digest_without_explicit_tag() {
        let refs = vec!["latest".to_string()];
        assert!(should_include_digest(None, &refs));
    }

    #[test]
    fn test_has_explicit_tag() {
        assert!(!has_explicit_tag("repo/app"));
        assert!(!has_explicit_tag("localhost:5000/repo/app"));
        assert!(has_explicit_tag("repo/app:v1"));
        assert!(has_explicit_tag("localhost:5000/repo/app:v1"));
        assert!(!has_explicit_tag("repo/app@sha256:1234"));
        assert!(has_explicit_tag("repo/app:v1@sha256:1234"));
    }

    #[test]
    fn test_has_explicit_registry() {
        assert!(!has_explicit_registry("repo/app"));
        assert!(has_explicit_registry("my.ns/team/app"));
        assert!(!has_explicit_registry("repo/app:v1"));
        assert!(has_explicit_registry("ghcr.io/acme/app"));
        assert!(has_explicit_registry("ghcr.io/acme/app:v1"));
        assert!(has_explicit_registry("localhost:5000/acme/app:v1"));
        assert!(has_explicit_registry("ghcr.io/acme/app@sha256:1234"));
    }

    #[test]
    fn test_strip_explicit_tag() {
        assert_eq!(strip_explicit_tag("repo/app"), "repo/app");
        assert_eq!(strip_explicit_tag("my.ns/team/app"), "my.ns/team/app");
        assert_eq!(
            strip_explicit_tag("localhost:5000/repo/app"),
            "localhost:5000/repo/app"
        );
        assert_eq!(strip_explicit_tag("repo/app:v1"), "repo/app");
        assert_eq!(
            strip_explicit_tag("localhost:5000/repo/app:v1"),
            "localhost:5000/repo/app"
        );
        assert_eq!(strip_explicit_tag("repo/app@sha256:1234"), "repo/app");
        assert_eq!(strip_explicit_tag("repo/app:v1@sha256:1234"), "repo/app");
    }

    #[test]
    fn test_implicit_push_repo_path_preserved() {
        let ref_name = strip_explicit_tag("my.ns/team/app");
        let target = parse_image_ref("127.0.0.1:8968", ref_name, Some("latest")).unwrap();
        assert_eq!(target.repository(), "my.ns/team/app");
    }

    #[test]
    fn test_registry_qualified_push_ref_is_not_prefixed_twice() {
        let parsed = "ghcr.io/acme/app:v1"
            .parse::<oci_spec::distribution::Reference>()
            .unwrap();
        let normalized = format!("{}:{}", parsed.repository(), parsed.tag().unwrap());
        let target = parse_image_ref(parsed.registry(), normalized, None::<String>).unwrap();
        assert_eq!(target.whole(), "ghcr.io/acme/app:v1");
    }

    #[test]
    fn test_blob_upload_strategy_prefers_monolithic_for_small_blobs() {
        assert_eq!(blob_upload_strategy(0), BlobUploadStrategy::Monolithic);
        assert_eq!(
            blob_upload_strategy(16 * 1024 * 1024),
            BlobUploadStrategy::Monolithic
        );
    }

    #[test]
    fn test_blob_upload_strategy_uses_chunked_for_large_blobs() {
        assert_eq!(
            blob_upload_strategy(16 * 1024 * 1024 + 1),
            BlobUploadStrategy::Chunked
        );
        assert_eq!(
            blob_upload_strategy(3 * 1024 * 1024 * 1024),
            BlobUploadStrategy::Chunked
        );
    }
}
