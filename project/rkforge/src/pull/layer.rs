use crate::pull::media::get_media_type;
use crate::storage::{DigestExt, ultimate_blob_path};
use anyhow::{Context, bail};
use futures::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use iterator_ext::IteratorExt;
use oci_client::Client;
use oci_client::client::BlobResponse;
use oci_client::manifest::{OciDescriptor, OciImageManifest};
use oci_spec::distribution::Reference;
use sha2::{Digest, Sha256};
use std::iter;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;

const DEFAULT_PULL_CONCURRENCY: usize = 3;
const DEFAULT_PULL_RETRIES: usize = 3;

pub async fn pull_layers(
    client: &Client,
    image_ref: &Reference,
    manifest: &OciImageManifest,
    no_cache: bool,
    quiet: bool,
) -> anyhow::Result<Vec<PathBuf>> {
    if !no_cache {
        remove_invalid_cached_layer_blobs(manifest).await?;
    }

    let to_download = manifest
        .layers
        .iter()
        .chain(iter::once(&manifest.config))
        .map(Ok)
        .try_filter(|layer| Ok(no_cache || !ultimate_blob_path(&layer.digest)?.exists()))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let new_blob_paths = to_download
        .iter()
        .map(|descriptor| ultimate_blob_path(&descriptor.digest))
        .collect::<anyhow::Result<Vec<_>>>()?;

    let semaphore = Arc::new(Semaphore::new(pull_concurrency_limit()));
    let tasks = to_download
        .into_iter()
        .map(|descriptor| {
            let client = client.clone();
            let image_ref = image_ref.clone();
            let descriptor = descriptor.clone();
            let semaphore = semaphore.clone();

            tokio::spawn(async move {
                let _permit = semaphore
                    .acquire_owned()
                    .await
                    .with_context(|| "Failed to acquire layer pull permit")?;
                pull_and_unpack_layer(&client, &image_ref, &descriptor, no_cache, quiet).await
            })
        })
        .collect::<Vec<_>>();

    if let Err(err) = wait_for_layer_tasks(tasks).await {
        if let Err(cleanup_err) = remove_cached_blobs(&new_blob_paths).await {
            tracing::warn!(
                error = ?cleanup_err,
                "failed to remove blobs created by failed image pull"
            );
        }
        return Err(err);
    }

    let layers = manifest
        .layers
        .iter()
        .map(|layer| ultimate_blob_path(&layer.digest))
        .collect::<anyhow::Result<Vec<_>>>()?;
    ensure_unpacked_layer_dirs(&layers).await?;
    Ok(layers)
}

async fn remove_invalid_cached_layer_blobs(manifest: &OciImageManifest) -> anyhow::Result<()> {
    for layer in &manifest.layers {
        let path = ultimate_blob_path(&layer.digest)?;
        let metadata = match tokio::fs::metadata(&path).await {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => {
                return Err(err)
                    .with_context(|| format!("Failed to stat cached layer {}", path.display()));
            }
        };
        if !metadata.is_dir() {
            remove_cached_blob(&path).await?;
        }
    }
    Ok(())
}

fn pull_concurrency_limit() -> usize {
    parse_positive_env_usize("RKFORGE_PULL_CONCURRENCY", DEFAULT_PULL_CONCURRENCY)
}

fn pull_retry_attempts() -> usize {
    parse_positive_env_usize("RKFORGE_PULL_RETRIES", DEFAULT_PULL_RETRIES)
}

fn parse_positive_env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

async fn remove_cached_blobs(paths: &[PathBuf]) -> anyhow::Result<()> {
    for path in paths {
        remove_cached_blob(path).await?;
    }
    Ok(())
}

async fn wait_for_layer_tasks(tasks: Vec<JoinHandle<anyhow::Result<()>>>) -> anyhow::Result<()> {
    let results = futures::future::join_all(tasks).await;
    for result in results {
        result
            .with_context(|| "Failed to join layer download task")?
            .with_context(|| "Failed to pull and unpack layer")?;
    }
    Ok(())
}

async fn ensure_unpacked_layer_dirs(layers: &[PathBuf]) -> anyhow::Result<()> {
    for layer in layers {
        let metadata = tokio::fs::metadata(layer)
            .await
            .with_context(|| format!("Pulled layer is missing: {}", layer.display()))?;
        if !metadata.is_dir() {
            bail!(
                "Pulled layer is not an unpacked directory: {}",
                layer.display()
            );
        }
    }
    Ok(())
}

async fn pull_and_unpack_layer(
    client: &Client,
    image_ref: &Reference,
    descriptor: &OciDescriptor,
    no_cache: bool,
    quiet: bool,
) -> anyhow::Result<()> {
    let digest = &descriptor.digest;

    let temp_dir = TempDir::new().with_context(|| "Failed to create temporary directory")?;
    let packed_blob_path = temp_dir.path().join(digest);
    let ultimate_blob_path = ultimate_blob_path(digest)?;

    pull_layer(client, image_ref, descriptor, &packed_blob_path, quiet).await?;

    if no_cache {
        remove_cached_blob(&ultimate_blob_path).await?;
    }

    if no_cache || !ultimate_blob_path.exists() {
        match unpack_layer(descriptor, &packed_blob_path, &ultimate_blob_path).await {
            Ok(()) => {}
            Err(err) => {
                let _ = remove_cached_blob(&ultimate_blob_path).await;
                return Err(err);
            }
        }
    }

    Ok(())
}

async fn remove_cached_blob(path: &Path) -> anyhow::Result<()> {
    let metadata = match tokio::fs::symlink_metadata(path).await {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => {
            return Err(err)
                .with_context(|| format!("Failed to stat cached blob {}", path.display()));
        }
    };

    if metadata.file_type().is_dir() {
        tokio::fs::remove_dir_all(path).await.with_context(|| {
            format!("Failed to remove cached blob directory {}", path.display())
        })?;
    } else {
        tokio::fs::remove_file(path)
            .await
            .with_context(|| format!("Failed to remove cached blob {}", path.display()))?;
    }
    Ok(())
}

async fn pull_layer(
    client: &Client,
    image_ref: &Reference,
    descriptor: &OciDescriptor,
    dest: impl AsRef<Path>,
    quiet: bool,
) -> anyhow::Result<()> {
    let dest = dest.as_ref();
    let digest = descriptor.digest.split_digest()?;

    let progress_bar = new_progress_bar(descriptor.size as u64, digest, quiet);
    let attempts = pull_retry_attempts();

    for attempt in 1..=attempts {
        match pull_layer_attempt(client, image_ref, descriptor, dest, &progress_bar).await {
            Ok(()) => {
                progress_bar.finish_with_message(format!("Downloaded layer {digest}"));
                return Ok(());
            }
            Err(err) if attempt < attempts => {
                let downloaded = downloaded_blob_size(dest).await.unwrap_or(0);
                progress_bar.set_position(downloaded.min(descriptor.size as u64));
                progress_bar.set_message(format!(
                    "Retrying layer {digest} ({}/{attempts})",
                    attempt + 1
                ));
                tracing::warn!(
                    error = ?err,
                    layer = %descriptor.digest,
                    attempt,
                    attempts,
                    "layer pull attempt failed; retrying"
                );
                tokio::time::sleep(retry_delay(attempt)).await;
            }
            Err(err) => return Err(err),
        }
    }

    unreachable!("pull retry loop always returns")
}

async fn pull_layer_attempt(
    client: &Client,
    image_ref: &Reference,
    descriptor: &OciDescriptor,
    dest: &Path,
    progress_bar: &ProgressBar,
) -> anyhow::Result<()> {
    let expected_size = descriptor.size as u64;
    let mut offset = downloaded_blob_size(dest).await?;
    if offset > expected_size {
        remove_cached_blob(dest).await?;
        offset = 0;
    }

    if offset == expected_size && expected_size != 0 {
        match verify_downloaded_blob(dest, &descriptor.digest).await {
            Ok(()) => {
                progress_bar.set_position(expected_size);
                return Ok(());
            }
            Err(err) => {
                let _ = remove_cached_blob(dest).await;
                return Err(err);
            }
        }
    }

    progress_bar.set_position(offset);
    let response = client
        .pull_blob_stream_partial(image_ref, descriptor, offset, None)
        .await
        .with_context(|| format!("Failed to pull layer blob {}", descriptor.digest))?;

    let (stream, append) = match response {
        BlobResponse::Full(stream) => {
            if offset > 0 {
                progress_bar.set_position(0);
            }
            (stream, false)
        }
        BlobResponse::Partial(stream) => (stream, true),
    };

    let file = if append {
        tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dest)
            .await
            .with_context(|| format!("Failed to open layer file: {}", dest.display()))?
    } else {
        tokio::fs::File::create(dest)
            .await
            .with_context(|| format!("Failed to create layer file: {}", dest.display()))?
    };
    let mut writer = BufWriter::new(file);
    let mut stream = stream.stream;

    while let Some(bytes) = stream.next().await {
        let bytes = bytes.with_context(|| format!("Failed to read layer {}", descriptor.digest))?;
        writer
            .write_all(&bytes)
            .await
            .with_context(|| format!("Failed to write layer file: {}", dest.display()))?;
        progress_bar.inc(bytes.len() as u64);
    }

    writer
        .flush()
        .await
        .with_context(|| format!("Failed to flush layer file: {}", dest.display()))?;

    let metadata = std::fs::metadata(dest)?;
    if metadata.len() != expected_size {
        bail!(
            "Downloaded layer size mismatch for {}. Expected {}, but got {}",
            descriptor.digest,
            expected_size,
            metadata.len()
        );
    }
    if let Err(err) = verify_downloaded_blob(dest, &descriptor.digest).await {
        let _ = remove_cached_blob(dest).await;
        return Err(err);
    }
    Ok(())
}

async fn downloaded_blob_size(path: &Path) -> anyhow::Result<u64> {
    match tokio::fs::metadata(path).await {
        Ok(metadata) => Ok(metadata.len()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(err) => {
            Err(err).with_context(|| format!("Failed to stat layer download {}", path.display()))
        }
    }
}

async fn verify_downloaded_blob(path: &Path, digest: &str) -> anyhow::Result<()> {
    let path = path.to_owned();
    let digest = digest.to_string();
    let digest_for_context = digest.clone();
    tokio::task::spawn_blocking(move || verify_downloaded_blob_sync(&path, &digest))
        .await?
        .with_context(|| format!("Failed to verify downloaded layer {}", digest_for_context))
}

fn verify_downloaded_blob_sync(path: &Path, digest: &str) -> anyhow::Result<()> {
    let expected = digest
        .strip_prefix("sha256:")
        .with_context(|| format!("Unsupported layer digest algorithm: {digest}"))?;
    let mut file = std::fs::File::open(path)
        .with_context(|| format!("Failed to open downloaded layer {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let read = std::io::Read::read(&mut file, &mut buffer)
            .with_context(|| format!("Failed to read downloaded layer {}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let actual = format!("{:x}", hasher.finalize());
    if actual != expected {
        bail!(
            "Downloaded layer digest mismatch for {}. Expected sha256:{}, got sha256:{}",
            path.display(),
            expected,
            actual
        );
    }
    Ok(())
}

fn retry_delay(attempt: usize) -> Duration {
    let millis = 500_u64.saturating_mul(1_u64 << (attempt.saturating_sub(1).min(4)));
    Duration::from_millis(millis.min(8_000))
}

async fn unpack_layer(
    descriptor: &OciDescriptor,
    src: impl AsRef<Path>,
    dst: impl AsRef<Path>,
) -> anyhow::Result<()> {
    let media_type = get_media_type(&descriptor.media_type);

    let src = src.as_ref().to_owned();
    let dst = dst.as_ref().to_owned();
    let tmp_dst = temporary_unpack_path(&dst)?;

    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let unpack_result = media_type.unpack(&src, &tmp_dst);
        match unpack_result {
            Ok(()) => std::fs::rename(&tmp_dst, &dst)
                .with_context(|| format!("Failed to move unpacked layer to {}", dst.display())),
            Err(err) => {
                remove_path_best_effort(&tmp_dst);
                Err(err)
            }
        }
    })
    .await?
    .with_context(|| format!("Failed to unpack layer {}", descriptor.digest))
}

fn temporary_unpack_path(dst: &Path) -> anyhow::Result<PathBuf> {
    let parent = dst
        .parent()
        .with_context(|| format!("Blob path has no parent: {}", dst.display()))?;
    let file_name = dst
        .file_name()
        .with_context(|| format!("Blob path has no file name: {}", dst.display()))?
        .to_string_lossy();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    Ok(parent.join(format!(".{file_name}.tmp-{}-{nanos}", std::process::id())))
}

fn remove_path_best_effort(path: &Path) {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return;
    };
    if metadata.file_type().is_dir() {
        let _ = std::fs::remove_dir_all(path);
    } else {
        let _ = std::fs::remove_file(path);
    }
}

fn new_progress_bar(total_size: u64, digest: impl AsRef<str>, quiet: bool) -> ProgressBar {
    let progress_bar = if quiet {
        ProgressBar::hidden()
    } else {
        ProgressBar::new(total_size)
    };

    if !quiet {
        progress_bar.set_style(
            ProgressStyle::default_bar()
                .template(
                    "{msg} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec})",
                )
                .unwrap()
                .progress_chars("#>-"),
        );
        progress_bar.set_message(format!("Downloading layer {}", digest.as_ref()));
    }
    progress_bar
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_unpacked_layer_dirs, parse_positive_env_usize, remove_cached_blobs, retry_delay,
        wait_for_layer_tasks,
    };

    #[tokio::test]
    async fn wait_for_layer_tasks_propagates_inner_error() {
        let tasks = vec![tokio::spawn(async { anyhow::bail!("unpack failed") })];

        let err = wait_for_layer_tasks(tasks).await.unwrap_err();

        assert!(format!("{err:?}").contains("unpack failed"));
    }

    #[tokio::test]
    async fn ensure_unpacked_layer_dirs_rejects_missing_layer() {
        let temp_dir = tempfile::tempdir().unwrap();
        let missing_layer = temp_dir.path().join("missing-layer");

        let err = ensure_unpacked_layer_dirs(&[missing_layer])
            .await
            .unwrap_err();

        assert!(err.to_string().contains("Pulled layer is missing"));
    }

    #[tokio::test]
    async fn remove_cached_blobs_removes_files_and_dirs() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file = temp_dir.path().join("config");
        let dir = temp_dir.path().join("layer");
        std::fs::write(&file, "config").unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("file"), "layer").unwrap();

        remove_cached_blobs(&[file.clone(), dir.clone()])
            .await
            .unwrap();

        assert!(!file.exists());
        assert!(!dir.exists());
    }

    #[test]
    fn retry_delay_is_capped() {
        assert_eq!(retry_delay(1), std::time::Duration::from_millis(500));
        assert_eq!(retry_delay(99), std::time::Duration::from_millis(8_000));
    }

    #[test]
    fn parse_positive_env_usize_uses_default_for_invalid_values() {
        assert_eq!(parse_positive_env_usize("__RKFORGE_TEST_MISSING", 7), 7);
    }
}
