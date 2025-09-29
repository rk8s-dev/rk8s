mod pusher;

use crate::config::auth::AuthConfig;
use crate::push::pusher::{PushTask, Pusher};
use crate::rt::block_on;
use crate::storage::{DigestExt, parse_image_ref};
use anyhow::Context;
use clap::Parser;
use oci_client::client::{ClientConfig, ImageLayer};
use oci_client::manifest::{OciImageIndex, OciManifest};
use oci_client::secrets::RegistryAuth;
use oci_client::{Client, client};
use oci_spec::distribution::Reference;
use std::path::Path;
use tokio::io::AsyncReadExt;

macro_rules! from_oci_blob {
    ($output:ty, $path:expr, $descriptor:expr) => {{
        let mut buffer = Vec::new();
        let mut file = tokio::fs::File::open($path).await?;
        file.read_to_end(&mut buffer).await?;

        <$output>::new(
            buffer,
            $descriptor.media_type.clone(),
            $descriptor.annotations.clone(),
        )
    }};
}

#[derive(Parser, Debug)]
pub struct PushArgs {
    /// Image reference
    image_ref: String,
    /// Image path (default current directory)
    #[arg(long)]
    path: Option<String>,
    #[arg(long)]
    url: Option<String>,
}

pub fn push(args: PushArgs) -> anyhow::Result<()> {
    let auth_config = AuthConfig::load()?;

    let url = auth_config.resolve_url(args.url);

    let auth_method = auth_config
        .find_entry_by_url(&url)
        .map(|entry| RegistryAuth::Bearer(entry.pat.clone()))
        .unwrap_or(RegistryAuth::Anonymous);

    let client_config = ClientConfig {
        protocol: client::ClientProtocol::Http,
        ..Default::default()
    };
    let client = Client::new(client_config);

    let image_ref = parse_image_ref(url, args.image_ref, None::<String>)?;
    let path = args.path.unwrap_or(".".to_string());

    block_on(async move { push_image(&client, &image_ref, &auth_method, path).await })?
}

pub async fn push_image(
    client: &Client,
    image_ref: &Reference,
    auth_method: &RegistryAuth,
    path: impl AsRef<Path>,
) -> anyhow::Result<()> {
    let dir = path.as_ref();

    let image_index_path = dir.join("index.json");
    let image_index = serde_json::from_str::<OciImageIndex>(
        &tokio::fs::read_to_string(&image_index_path)
            .await
            .with_context(|| format!("Failed to read from {}", image_index_path.display()))?,
    )?;

    let dir = dir.join("blobs/sha256");

    let mut tasks = Vec::new();
    for entry in image_index.manifests {
        let digest = entry.digest.split_digest()?;

        let manifest_path = dir.join(digest);
        let manifest = serde_json::from_str::<OciManifest>(
            &tokio::fs::read_to_string(&manifest_path)
                .await
                .with_context(|| format!("Failed to read from {}", manifest_path.display()))?,
        )?;
        let manifest = match manifest {
            OciManifest::Image(manifest) => manifest,
            OciManifest::ImageIndex(_) => anyhow::bail!("Image indexes are not supported yet"),
        };

        let descriptors = &manifest.layers;
        let mut layers = Vec::new();

        for descriptor in descriptors {
            let layer_path = dir.join(descriptor.digest.split_digest()?);
            let layer = from_oci_blob!(ImageLayer, layer_path, descriptor);
            layers.push(layer);
        }

        let config_path = dir.join(manifest.config.digest.split_digest()?);
        let config = from_oci_blob!(client::Config, config_path, manifest.config);

        let image_ref = image_ref.clone();
        let auth_method = auth_method.clone();
        let client = client.clone();
        let task = PushTask::new(
            digest,
            Box::pin(async move {
                client
                    .push(&image_ref, &layers, config, &auth_method, Some(manifest))
                    .await
            }),
        );
        tasks.push(task);
    }

    let pusher = Pusher::new(tasks);
    pusher.push_all().await?;
    Ok(())
}
