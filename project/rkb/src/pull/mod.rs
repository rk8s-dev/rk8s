mod downloader;
mod layer;
mod media;

use crate::config::auth::AuthConfig;
use crate::pull::layer::pull_layers;
use crate::rt::block_on;
use crate::storage::{parse_image_ref, write_manifest};
use anyhow::Context;
use clap::Parser;
use oci_client::client::ClientConfig;
use oci_client::manifest::OciManifest;
use oci_client::secrets::RegistryAuth;
use oci_client::{Client, client};
use std::path::PathBuf;

#[derive(Parser, Debug)]
pub struct PullArgs {
    /// Image reference. (e.g "ubuntu:latest" or "me.org/ubuntu:latest")
    image_ref: String,
    /// URL of the distribution server (optional if only one server is configured)
    #[arg(long)]
    url: Option<String>,
}

pub fn pull(args: PullArgs) -> anyhow::Result<()> {
    pull_or_get_image(args.image_ref, args.url)?;
    Ok(())
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
pub fn pull_or_get_image(
    image_ref: impl AsRef<str>,
    url: Option<impl AsRef<str>>,
) -> anyhow::Result<(PathBuf, Vec<PathBuf>)> {
    let image_ref = image_ref.as_ref();

    let auth_config = AuthConfig::load()?;

    let url = auth_config.resolve_url(url);

    let auth_method = match auth_config.find_entry_by_url(&url) {
        Ok(entry) => RegistryAuth::Bearer(entry.pat.clone()),
        Err(_) => RegistryAuth::Anonymous,
    };

    let client_config = ClientConfig {
        protocol: client::ClientProtocol::Http,
        ..Default::default()
    };
    let client = Client::new(client_config);

    let image_ref = parse_image_ref(url, image_ref, None::<String>)?;
    block_on(async move {
        let (manifest, digest) = client
            .pull_manifest(&image_ref, &auth_method)
            .await
            .with_context(|| "Failed to pull manifest")?;

        let layers = match &manifest {
            OciManifest::Image(manifest) => pull_layers(&client, &image_ref, manifest).await,
            OciManifest::ImageIndex(_) => anyhow::bail!("Image indexes are not supported yet"),
        }?;

        let manifest_path = write_manifest(&image_ref, &manifest, &digest).await?;
        Ok((manifest_path, layers))
    })?
}
