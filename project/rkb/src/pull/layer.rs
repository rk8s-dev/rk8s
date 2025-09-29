use crate::pull::downloader::LayerDownloadWrapper;
use crate::pull::media::get_media_type;
use crate::storage::{DigestExt, ultimate_blob_path};
use anyhow::Context;
use indicatif::{ProgressBar, ProgressStyle};
use iterator_ext::IteratorExt;
use oci_client::Client;
use oci_client::manifest::{OciDescriptor, OciImageManifest};
use oci_spec::distribution::Reference;
use std::iter;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use tokio::io::AsyncWriteExt;

pub async fn pull_layers(
    client: &Client,
    image_ref: &Reference,
    manifest: &OciImageManifest,
) -> anyhow::Result<Vec<PathBuf>> {
    let to_download = manifest
        .layers
        .iter()
        .chain(iter::once(&manifest.config))
        .map(Ok)
        .try_filter(|layer| Ok(!ultimate_blob_path(&layer.digest)?.exists()))
        .collect::<anyhow::Result<Vec<_>>>()?;

    let tasks = to_download.into_iter().map(|descriptor| {
        let client = client.clone();
        let image_ref = image_ref.clone();
        let descriptor = descriptor.clone();

        tokio::spawn(async move { pull_and_unpack_layer(&client, &image_ref, &descriptor).await })
    });

    futures::future::try_join_all(tasks).await?;

    let layers = manifest
        .layers
        .iter()
        .map(|layer| ultimate_blob_path(&layer.digest))
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(layers)
}

async fn pull_and_unpack_layer(
    client: &Client,
    image_ref: &Reference,
    descriptor: &OciDescriptor,
) -> anyhow::Result<()> {
    let digest = &descriptor.digest;

    let temp_dir = TempDir::new().with_context(|| "Failed to create temporary directory")?;
    let packed_blob_path = temp_dir.path().join(digest);
    let ultimate_blob_path = ultimate_blob_path(digest)?;

    pull_layer(client, image_ref, descriptor, &packed_blob_path).await?;

    if !ultimate_blob_path.exists() {
        unpack_layer(descriptor, &packed_blob_path, &ultimate_blob_path).await?;
    }

    Ok(())
}

async fn pull_layer(
    client: &Client,
    image_ref: &Reference,
    descriptor: &OciDescriptor,
    dest: impl AsRef<Path>,
) -> anyhow::Result<()> {
    let dest = dest.as_ref();
    let digest = descriptor.digest.split_digest()?;

    let progress_bar = new_progress_bar(descriptor.size as u64, digest);

    let raw_layer_file = tokio::fs::File::create(dest)
        .await
        .with_context(|| format!("Failed to create layer file: {}", dest.display()))?;
    let mut writer = LayerDownloadWrapper::new(raw_layer_file, progress_bar.clone());

    client
        .pull_blob(image_ref, descriptor, &mut writer)
        .await
        .with_context(|| "Failed to pull layer blob of {}")?;

    writer.flush().await?;
    progress_bar.finish_with_message(format!("Downloaded layer {digest}"));

    let metadata = std::fs::metadata(dest)?;
    if metadata.len() != descriptor.size as u64 {
        anyhow::bail!(
            "Downloaded layer size mismatch. Expected {}, but got {}",
            descriptor.size,
            metadata.len()
        );
    }
    Ok(())
}

async fn unpack_layer(
    descriptor: &OciDescriptor,
    src: impl AsRef<Path>,
    dst: impl AsRef<Path>,
) -> anyhow::Result<()> {
    let media_type = get_media_type(&descriptor.media_type);

    let src = src.as_ref().to_owned();
    let dst = dst.as_ref().to_owned();

    tokio::task::spawn_blocking(move || -> anyhow::Result<()> { media_type.unpack(&src, &dst) })
        .await?
        .with_context(|| format!("Failed to unpack layer {}", descriptor.digest))
}

fn new_progress_bar(total_size: u64, digest: impl AsRef<str>) -> ProgressBar {
    let progress_bar = ProgressBar::new(total_size);

    progress_bar.set_style(ProgressStyle::default_bar()
        .template("{msg} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec})")
        .unwrap()
        .progress_chars("#>-"));
    progress_bar.set_message(format!("Downloading layer {}", digest.as_ref()));
    progress_bar
}
