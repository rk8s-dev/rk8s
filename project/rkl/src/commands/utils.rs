use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, bail};
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

pub fn handle_oci_image<P: AsRef<Path>>(image: P, _name: String) -> Result<ImageConfiguration> {
    // read the image's manifest
    let index = oci_spec::image::ImageIndex::from_file(image.as_ref().join("index.json"))
        .map_err(|e| anyhow!("failed to get index.json in image dir: {}", e))?;

    let image_manifest_descriptor = index
        .manifests()
        .iter()
        .find(|descriptor| matches!(descriptor.media_type(), MediaType::ImageManifest))
        .with_context(|| anyhow!("Failed to get image manifest descriptor"))?;

    let image_manifest_hash = image_manifest_descriptor
        .as_digest_sha256()
        .unwrap_or_default();

    debug!("image_manifest_hash: {image_manifest_hash}");

    if let Some(digest) = image_manifest_descriptor.as_digest_sha256() {
        debug!("digest: {digest}");
        let bundle_path = PathBuf::from(format!("{RKL_IMAGE_REGISTRY}/{digest}"));
        if bundle_path.exists() {
            let image_path = image.as_ref().join("blobs/sha256");

            let image_manifest_path = image_path.join(digest);
            let image_manifest = ImageManifest::from_file(image_manifest_path)
                .with_context(|| "Failed to read manifest.json")?;

            let image_config_hash = image_manifest
                .config()
                .as_digest_sha256()
                .with_context(|| "Failed to get digest from config descriptor")?;
            let image_config_path = image_path.join(image_config_hash);
            let image_config = ImageConfiguration::from_file(&image_config_path)
                .with_context(|| "Failed to read config.json")?;
            return Ok(image_config);
        }
    }

    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(bundle::convert_image_to_bundle(
            image,
            format!("{RKL_IMAGE_REGISTRY}/{image_manifest_hash}"),
        ))
}

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
