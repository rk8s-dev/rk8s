use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Ok, Result, anyhow, bail};
use oci_spec::image::{ImageConfiguration, ImageManifest, MediaType};
use thiserror::Error;
use tracing::debug;

use crate::bundle;

const RKL_IMAGE_REGISTRY: &str = "/var/lib/rkl/registry";

pub enum ImageType {
    Bundle,
    OCIImage,
}

#[derive(Error, Debug)]
pub enum UtilsError {
    #[error("invalid image path")]
    InvalidImagePath,
}

pub fn get_manifest_from_image_ref(image_ref: impl AsRef<str>) -> Result<String> {
    let (manifest_path, _) = rkb::pull::pull_or_get_image(image_ref, None::<&str>)
        .map_err(|e| anyhow!("failed to pull image: {e}"))?;

    manifest_path
        .file_name()
        .map(|str| str.to_str().unwrap_or("unknown").to_string())
        .with_context(|| anyhow!("failed to get manifest hash"))
}

pub fn get_bundle_from_image_ref(image_ref: impl AsRef<str>) -> Result<PathBuf> {
    let manifest_hash = get_manifest_from_image_ref(image_ref)?;
    Ok(PathBuf::from(format!(
        "{RKL_IMAGE_REGISTRY}/{manifest_hash}"
    )))
}

#[allow(dead_code)]
pub fn get_bundle_from_path<P: AsRef<Path>>(image_path: P) -> Result<PathBuf> {
    let index = oci_spec::image::ImageIndex::from_file(image_path.as_ref().join("index.json"))
        .map_err(|e| anyhow!("failed to get index.json in image dir: {}", e))?;

    let image_manifest_descriptor = index
        .manifests()
        .iter()
        .find(|descriptor| matches!(descriptor.media_type(), MediaType::ImageManifest))
        .with_context(|| anyhow!("Failed to get image manifest descriptor"))?;

    let image_manifest_hash = image_manifest_descriptor
        .as_digest_sha256()
        .unwrap_or_default();

    Ok(PathBuf::from(format!(
        "{RKL_IMAGE_REGISTRY}/{image_manifest_hash}"
    )))
}

/// pull image from rkb's implementation
pub fn handle_oci_image(image_ref: impl AsRef<str>, _name: String) -> Result<ImageConfiguration> {
    let (manifest_path, layers) = rkb::pull::pull_or_get_image(image_ref, None::<&str>)
        .map_err(|e| anyhow!("failed to pull image: {e}"))?;

    debug!("get manifest_path: {manifest_path:?}");
    debug!("layers: {layers:?}");

    let manifest_hash = manifest_path
        .file_name()
        .map(|str| str.to_str().unwrap_or("unknown").to_string())
        .with_context(|| anyhow!("failed to get manifest hash"))?;

    let bundle_path = format!("{RKL_IMAGE_REGISTRY}/{manifest_hash}");
    let config = get_image_config(&manifest_path)?;

    if PathBuf::from(&bundle_path).exists() {
        return Ok(config);
    }

    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(bundle::mount_and_copy_bundle(bundle_path, &layers))?;
    Ok(config)
}

pub fn get_image_config(manifest_path: impl AsRef<Path>) -> Result<ImageConfiguration> {
    let digest = ImageManifest::from_file(manifest_path.as_ref())?
        .config()
        .digest()
        .digest()
        .to_string();

    let dir = manifest_path.as_ref().parent().unwrap().join(digest);
    ImageConfiguration::from_file(dir).map_err(|e| anyhow!("failed to get image config: {e}"))
}

pub fn determine_image(target: impl AsRef<str>) -> Result<ImageType> {
    match PathBuf::from(target.as_ref()).exists() {
        true => Ok(ImageType::Bundle),
        false => Ok(ImageType::OCIImage),
    }
}

#[allow(dead_code)]
pub fn determine_image_path<P: AsRef<Path>>(target: P) -> Result<ImageType> {
    if !target.as_ref().is_dir() {
        bail!("invalid image path")
    }

    let path = fs::canonicalize(target.as_ref())?;

    // check if if is bundle
    if path.join("config.json").exists() && path.join("rootfs").is_dir() {
        return Ok(ImageType::Bundle);
    }

    if path.join("index.json").exists()
        && path.join("blobs").is_dir()
        && path.join("oci-layout").exists()
    {
        return Ok(ImageType::OCIImage);
    }

    Err(UtilsError::InvalidImagePath.into())
}

pub fn parse_key_val(s: &str) -> Result<(String, String), String> {
    s.split_once("=")
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .ok_or_else(|| format!("invalid KEY=VALUE: '{}'", s))
}
