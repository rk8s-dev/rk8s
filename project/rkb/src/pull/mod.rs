mod downloader;
mod layer;
mod media;

use crate::config::auth::AuthConfig;
use crate::pull::layer::pull_layers;
use crate::rt::block_on;
use crate::storage::{parse_image_ref, write_manifest};
use crate::utils::cli::sudo_guard;
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
    sudo_guard(vec![(
        "AUTH_CONFIG_PATH",
        AuthConfig::current_config_path()?.display().to_string(),
    )])?;

    let auth_config_path = std::env::var("AUTH_CONFIG_PATH")?;
    pull_or_get_image(args.image_ref, args.url, &auth_config_path)?;
    Ok(())
}

pub fn pull_or_get_image(
    image_ref: impl AsRef<str>,
    url: Option<impl AsRef<str>>,
    config_path: impl AsRef<str>,
) -> anyhow::Result<Vec<PathBuf>> {
    let image_ref = image_ref.as_ref();

    let auth_config = AuthConfig::load_from(config_path.as_ref())?;

    let url = auth_config.resolve_url(url)?;

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

        write_manifest(&image_ref, &manifest, &digest).await?;
        Ok(layers)
    })?
}
