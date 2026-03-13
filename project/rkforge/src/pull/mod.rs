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

fn resolve_client_ref_auth(
    image_ref: &str,
    url: Option<String>,
    skip_tls_verify: bool,
) -> anyhow::Result<(Client, Reference, RegistryAuth)> {
    let auth_config = AuthConfig::load()?;
    let url = auth_config.resolve_url(url)?;
    resolve_ref_with_auth(&auth_config, &url, image_ref, skip_tls_verify)
}
