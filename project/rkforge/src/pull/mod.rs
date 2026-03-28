mod downloader;
mod layer;
pub mod media;

use crate::config::auth::AuthConfig;
use crate::pull::layer::pull_layers;
use crate::registry::{parse_registry_host_arg, resolve_client_ref_auth as resolve_ref_with_auth};
use crate::storage::write_manifest;
use anyhow::Context;
use anyhow::anyhow;
use clap::Parser;
use oci_client::Client;
use oci_client::manifest::OciManifest;
use oci_client::secrets::RegistryAuth;
use oci_spec::distribution::Reference;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::thread;
use tokio::runtime::{Handle, Runtime};

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

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

fn split_explicit_registry(image_ref: &str) -> anyhow::Result<Option<(String, String)>> {
    if !has_explicit_registry(image_ref) {
        return Ok(None);
    }

    let (registry, remainder) = image_ref
        .split_once('/')
        .ok_or_else(|| anyhow!("image reference is missing repository path: {image_ref}"))?;
    Ok(Some((
        crate::registry::parse_registry_host(registry)?,
        remainder.to_string(),
    )))
}

fn resolve_registry_and_image_ref(
    auth_config: &AuthConfig,
    image_ref: &str,
    url: Option<String>,
) -> anyhow::Result<(String, String)> {
    if let Some(url) = url {
        return Ok((auth_config.resolve_url(Some(url))?, image_ref.to_string()));
    }

    if let Some((registry, normalized_image_ref)) = split_explicit_registry(image_ref)? {
        return Ok((registry, normalized_image_ref));
    }

    Ok((
        auth_config.resolve_url(None::<String>)?,
        image_ref.to_string(),
    ))
}

#[derive(Parser, Debug)]
pub struct PullArgs {
    /// Image reference. (e.g "ubuntu:latest" or "me.org/ubuntu:latest")
    image_ref: String,
    /// Registry host in `host[:port]` format.
    #[arg(long, value_parser = parse_registry_host_arg)]
    url: Option<String>,
    /// Skip TLS certificate verification for HTTPS registry.
    #[arg(long)]
    skip_tls_verify: bool,
}

pub fn pull(args: PullArgs) -> anyhow::Result<()> {
    sync_pull_or_get_image_with_policy_and_output_with_tls(
        args.image_ref,
        args.url,
        false,
        false,
        args.skip_tls_verify,
    )?;
    Ok(())
}

pub fn sync_pull_or_get_image_with_policy(
    image_ref: impl AsRef<str>,
    url: Option<impl AsRef<str>>,
    no_cache: bool,
) -> anyhow::Result<(PathBuf, Vec<PathBuf>)> {
    sync_pull_or_get_image_with_policy_and_output_with_tls(image_ref, url, no_cache, false, false)
}

pub fn sync_pull_or_get_image_with_policy_and_output(
    image_ref: impl AsRef<str>,
    url: Option<impl AsRef<str>>,
    no_cache: bool,
    quiet: bool,
) -> anyhow::Result<(PathBuf, Vec<PathBuf>)> {
    sync_pull_or_get_image_with_policy_and_output_with_tls(image_ref, url, no_cache, quiet, false)
}

fn sync_pull_or_get_image_with_policy_and_output_with_tls(
    image_ref: impl AsRef<str>,
    url: Option<impl AsRef<str>>,
    no_cache: bool,
    quiet: bool,
    skip_tls_verify: bool,
) -> anyhow::Result<(PathBuf, Vec<PathBuf>)> {
    let image_ref = image_ref.as_ref();
    let url = url.map(|u| u.as_ref().to_string());
    let (client, image_ref, auth_method) =
        resolve_client_ref_auth(image_ref, url, skip_tls_verify)?;
    let do_pull = async move {
        let (manifest, digest) = client
            .pull_manifest(&image_ref, &auth_method)
            .await
            .map_err(|e| anyhow!("Failed to pull manifest: {e}"))?;

        let layers = match &manifest {
            OciManifest::Image(manifest) => {
                pull_layers(&client, &image_ref, manifest, no_cache, quiet).await
            }
            OciManifest::ImageIndex(_) => anyhow::bail!("Image indexes are not supported yet"),
        }?;

        let manifest_path = write_manifest(&image_ref, &manifest, &digest).await?;
        Ok((manifest_path, layers))
    };
    match Handle::try_current() {
        Ok(handle) => {
            let pull_or_get = thread::spawn(move || handle.block_on(do_pull));
            pull_or_get.join().map_err(|_| anyhow!("thread panicked"))?
        }
        Err(_) => {
            let rt = RUNTIME.get_or_init(|| {
                tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("Failed to build tokio runtime")
            });

            rt.block_on(do_pull)
        }
    }
}

/// Ensures an image is available locally, pulling any missing components.
///
/// This function implements a "local-first" strategy for image layers.
/// It iterates through the required layers, checking if they exist in the local cache.
/// Any layers not found locally will be pulled from the remote registry.
///
/// **Important**: This function will **always** fetch a fresh copy of the image manifest from
/// the registry to ensure the layer information is up-to-date. It does not use a cached manifest.
///
/// # Parameters
/// - `image_ref`: The reference of the image to retrieve, e.g., `ubuntu:latest`.
/// - `url`: An `Option` of registry url, it will be "resolved", please refer to [`AuthConfig::resolve_url`].
///
/// # Returns
///
/// On success, returns a `Result` containing a tuple with two elements:
/// 1. A `PathBuf` representing the local filesystem path to the newly fetched manifest.
/// 2. A `Vec<PathBuf>` containing the local filesystem paths to all the image's layers.
///
/// # Errors
///
/// Returns an error if the image reference is invalid, the pull from the registry fails,
/// or there are file system access issues.
pub fn sync_pull_or_get_image(
    image_ref: impl AsRef<str>,
    url: Option<impl AsRef<str>>,
) -> anyhow::Result<(PathBuf, Vec<PathBuf>)> {
    sync_pull_or_get_image_with_policy(image_ref, url, false)
}

pub async fn pull_or_get_image(
    image_ref: impl AsRef<str>,
    url: Option<impl AsRef<str>>,
) -> anyhow::Result<(PathBuf, Vec<PathBuf>)> {
    pull_or_get_image_with_policy(image_ref, url, false).await
}

pub async fn pull_or_get_image_with_policy(
    image_ref: impl AsRef<str>,
    url: Option<impl AsRef<str>>,
    no_cache: bool,
) -> anyhow::Result<(PathBuf, Vec<PathBuf>)> {
    pull_or_get_image_with_policy_and_tls(image_ref, url, no_cache, false).await
}

async fn pull_or_get_image_with_policy_and_tls(
    image_ref: impl AsRef<str>,
    url: Option<impl AsRef<str>>,
    no_cache: bool,
    skip_tls_verify: bool,
) -> anyhow::Result<(PathBuf, Vec<PathBuf>)> {
    let image_ref = image_ref.as_ref();
    let url = url.map(|u| u.as_ref().to_string());
    let (client, image_ref, auth_method) =
        resolve_client_ref_auth(image_ref, url, skip_tls_verify)?;
    let (manifest, digest) = client
        .pull_manifest(&image_ref, &auth_method)
        .await
        .with_context(|| "Failed to pull manifest")?;

    let layers = match &manifest {
        OciManifest::Image(manifest) => {
            pull_layers(&client, &image_ref, manifest, no_cache, false).await
        }
        OciManifest::ImageIndex(_) => anyhow::bail!("Image indexes are not supported yet"),
    }?;

    let manifest_path = write_manifest(&image_ref, &manifest, &digest).await?;
    Ok((manifest_path, layers))
}

/// Pull or get an image using an externally provided [AuthConfig] instead of
/// loading credentials from the local configuration file. This is used by rkl
/// when it receives registry credentials from rks.
pub fn sync_pull_or_get_image_with_config(
    image_ref: impl AsRef<str>,
    url: Option<impl AsRef<str>>,
    auth_config: &AuthConfig,
) -> anyhow::Result<(PathBuf, Vec<PathBuf>)> {
    let image_ref = image_ref.as_ref();
    let url = url.map(|u| u.as_ref().to_string());
    let (resolved_url, normalized_image_ref) =
        resolve_registry_and_image_ref(auth_config, image_ref, url)?;
    let (client, image_ref, auth_method) =
        resolve_ref_with_auth(auth_config, &resolved_url, &normalized_image_ref, false)?;
    let do_pull = async move {
        let (manifest, digest) = client
            .pull_manifest(&image_ref, &auth_method)
            .await
            .map_err(|e| anyhow!("Failed to pull manifest: {e}"))?;

        let layers = match &manifest {
            OciManifest::Image(manifest) => {
                pull_layers(&client, &image_ref, manifest, false, false).await
            }
            OciManifest::ImageIndex(_) => anyhow::bail!("Image indexes are not supported yet"),
        }?;

        let manifest_path = write_manifest(&image_ref, &manifest, &digest).await?;
        Ok((manifest_path, layers))
    };
    match Handle::try_current() {
        Ok(handle) => {
            let pull_or_get = thread::spawn(move || handle.block_on(do_pull));
            pull_or_get.join().map_err(|_| anyhow!("thread panicked"))?
        }
        Err(_) => {
            let rt = RUNTIME.get_or_init(|| {
                tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("Failed to build tokio runtime")
            });
            rt.block_on(do_pull)
        }
    }
}

/// Async variant of [sync_pull_or_get_image_with_config].
pub async fn pull_or_get_image_with_config(
    image_ref: impl AsRef<str>,
    url: Option<impl AsRef<str>>,
    auth_config: &AuthConfig,
) -> anyhow::Result<(PathBuf, Vec<PathBuf>)> {
    let image_ref = image_ref.as_ref();
    let url = url.map(|u| u.as_ref().to_string());
    let (resolved_url, normalized_image_ref) =
        resolve_registry_and_image_ref(auth_config, image_ref, url)?;
    let (client, image_ref, auth_method) =
        resolve_ref_with_auth(auth_config, &resolved_url, &normalized_image_ref, false)?;

    let (manifest, digest) = client
        .pull_manifest(&image_ref, &auth_method)
        .await
        .with_context(|| "Failed to pull manifest")?;

    let layers = match &manifest {
        OciManifest::Image(manifest) => {
            pull_layers(&client, &image_ref, manifest, false, false).await
        }
        OciManifest::ImageIndex(_) => anyhow::bail!("Image indexes are not supported yet"),
    }?;

    let manifest_path = write_manifest(&image_ref, &manifest, &digest).await?;
    Ok((manifest_path, layers))
}

fn resolve_client_ref_auth(
    image_ref: &str,
    url: Option<String>,
    skip_tls_verify: bool,
) -> anyhow::Result<(Client, Reference, RegistryAuth)> {
    let auth_config = AuthConfig::load()?;
    let (url, normalized_image_ref) = resolve_registry_and_image_ref(&auth_config, image_ref, url)?;
    resolve_ref_with_auth(&auth_config, &url, &normalized_image_ref, skip_tls_verify)
}

#[cfg(test)]
mod tests {
    use super::{has_explicit_registry, split_explicit_registry};

    #[test]
    fn split_explicit_registry_preserves_tag_and_repository() {
        let (registry, image_ref) = split_explicit_registry("ghcr.io/acme/app:v1")
            .unwrap()
            .unwrap();
        assert_eq!(registry, "ghcr.io");
        assert_eq!(image_ref, "acme/app:v1");
    }

    #[test]
    fn split_explicit_registry_preserves_digest() {
        let (registry, image_ref) = split_explicit_registry("localhost:5000/acme/app@sha256:1234")
            .unwrap()
            .unwrap();
        assert_eq!(registry, "localhost:5000");
        assert_eq!(image_ref, "acme/app@sha256:1234");
    }

    #[test]
    fn split_explicit_registry_ignores_implicit_registry_refs() {
        assert!(!has_explicit_registry("library/nginx:latest"));
        assert!(
            split_explicit_registry("library/nginx:latest")
                .unwrap()
                .is_none()
        );
    }
}
