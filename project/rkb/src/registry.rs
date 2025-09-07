use crate::config;
use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use oci_distribution::{
    Client, Reference,
    client::{self, ClientConfig},
    manifest::{self, OciManifest},
    secrets::RegistryAuth,
};
use std::{path::PathBuf, pin::Pin, task::Poll};
use tokio::io::{AsyncWrite, AsyncWriteExt, BufWriter};

#[derive(Debug)]
pub struct LocalImage {
    pub layers: Vec<PathBuf>,
    // TODO: Consider adding more fields?
}

pub async fn pull_or_get_image(image_tag: &str) -> Result<LocalImage> {
    let registry = &config::CONFIG.default_registry;

    let client_config = ClientConfig {
        protocol: client::ClientProtocol::Http,
        ..Default::default()
    };
    let client = Client::new(client_config);
    let image_ref = format!("{registry}/library/{image_tag}");
    let image_ref: Reference = image_ref.parse()?;
    let auth = RegistryAuth::Anonymous;

    let (manifest, _digest) = client
        .pull_manifest(&image_ref, &auth)
        .await
        .context("Failed to pull image manifest")?;

    match manifest {
        OciManifest::Image(image_manifest) => {
            let mut layers = Vec::new();
            let mut to_download = Vec::new();
            for layer in image_manifest.layers.iter() {
                let (_alg, digest) = layer
                    .digest
                    .rsplit_once(':')
                    .context("Invalid layer digest format")?;
                let path = config::CONFIG.layers_store_root.join(digest);
                if !path.exists() {
                    to_download.push(layer.clone());
                }
                layers.push(path);
            }

            if !to_download.is_empty() {
                let mut tasks = Vec::new();
                for layer in to_download {
                    let client = client.clone();
                    let image_ref = image_ref.clone();
                    tasks.push(tokio::spawn(async move {
                        pull_and_unpack_layer(&client, &image_ref, &layer).await
                    }));
                }
                for task in tasks {
                    // The first `?` handles task join errors, the second handles layer pull errors
                    task.await??;
                }
            }

            Ok(LocalImage { layers })
        }
        OciManifest::ImageIndex(_) => {
            // TODO: Handle multi-arch images if needed
            anyhow::bail!("Image indexes are not supported yet");
        }
    }
}

async fn pull_and_unpack_layer(
    client: &Client,
    image_ref: &Reference,
    layer_descriptor: &manifest::OciDescriptor,
) -> Result<()> {
    let (_alg, digest) = layer_descriptor.digest.rsplit_once(':').unwrap();
    let raw_layer_path = config::CONFIG
        .layers_store_root
        .join(layer_descriptor.digest.replace(':', "_"));
    if !raw_layer_path.exists() {
        let total_size = layer_descriptor.size as u64;
        let progress_bar = ProgressBar::new(total_size);
        progress_bar.set_style(ProgressStyle::default_bar()
            .template("{msg} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec})")?
            .progress_chars("#>-"));
        progress_bar.set_message(format!("Downloading layer {digest}"));
        let raw_layer = tokio::fs::File::create(&raw_layer_path)
            .await
            .with_context(|| {
                format!(
                    "Failed to create layer file at {:?}",
                    raw_layer_path.display()
                )
            })?;
        let mut writer = LayerDownloadWrapper::new(raw_layer, progress_bar.clone());
        client
            .pull_blob(image_ref, layer_descriptor, &mut writer)
            .await
            .context("Failed to pull layer blob")?;
        writer.flush().await?;
        progress_bar.finish_and_clear();
        assert_eq!(
            std::fs::metadata(&raw_layer_path)?.len(),
            layer_descriptor.size as u64
        );
        drop(writer);

        let layer_path = config::CONFIG.layers_store_root.join(digest);
        assert!(!layer_path.exists());
        tokio::fs::create_dir_all(&layer_path).await?;
        let media_type = get_media_type(&layer_descriptor.media_type);
        let raw_layer_path_clone = raw_layer_path.clone();
        let layer_path_clone = layer_path.clone();
        let layer_descriptor_clone = layer_descriptor.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            match media_type {
                MediaType::Tar => {
                    let tar_gz = std::fs::File::open(&raw_layer_path_clone)?;
                    let mut archive = tar::Archive::new(tar_gz);
                    archive.unpack(&layer_path_clone)?;
                }
                MediaType::TarGzip => {
                    let tar_gz = std::fs::File::open(&raw_layer_path_clone)?;
                    let decompressor = flate2::read::GzDecoder::new(tar_gz);
                    let mut archive = tar::Archive::new(decompressor);
                    archive.unpack(&layer_path_clone)?;
                }
                MediaType::Other => {
                    anyhow::bail!(
                        "Unsupported layer media type: {}",
                        layer_descriptor_clone.media_type
                    );
                }
            }
            Ok(())
        })
        .await??;

        tokio::fs::remove_file(&raw_layer_path).await?;
    }

    Ok(())
}

enum MediaType {
    Tar,
    TarGzip,
    Other,
}

fn get_media_type(media_type: &str) -> MediaType {
    if media_type.ends_with("tar+gzip") {
        MediaType::TarGzip
    } else if media_type.ends_with("tar") {
        MediaType::Tar
    } else {
        MediaType::Other
    }
}

struct LayerDownloadWrapper<W: AsyncWrite + Unpin> {
    inner: BufWriter<W>,
    progress_bar: ProgressBar,
}

impl<W: AsyncWrite + Unpin> LayerDownloadWrapper<W> {
    fn new(writer: W, progress_bar: ProgressBar) -> Self {
        Self {
            inner: BufWriter::new(writer),
            progress_bar,
        }
    }
}

impl<W: AsyncWrite + Unpin> AsyncWrite for LayerDownloadWrapper<W> {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::result::Result<usize, std::io::Error>> {
        let res = Pin::new(&mut self.inner).poll_write(cx, buf);
        if let Poll::Ready(Ok(n)) = &res {
            self.progress_bar.inc(*n as u64);
        }
        res
    }

    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<std::result::Result<(), std::io::Error>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<std::result::Result<(), std::io::Error>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}
