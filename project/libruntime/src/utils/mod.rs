use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, bail};
use oci_spec::image::{ImageConfiguration, ImageManifest, MediaType};
use thiserror::Error;
use tracing::debug;

use crate::bundle;
use crate::cri::config::ContainerConfigBuilder;
use common::ContainerSpec;

const RKL_IMAGE_REGISTRY: &str = "/var/lib/rkl/registry";
const RKL_BUNDLE_STORE: &str = "/var/lib/rkl/bundle";

pub trait ImagePuller {
    fn pull_or_get_image(&self, image_ref: &str) -> Result<(PathBuf, Vec<PathBuf>)>;
}

pub enum ImageType {
    Bundle,
    OCIImage,
}

#[derive(Error, Debug)]
pub enum UtilsError {
    #[error("invalid image path")]
    InvalidImagePath,
}

#[allow(unused)]
pub fn get_manifest_from_image_ref(
    puller: &impl ImagePuller,
    image_ref: impl AsRef<str>,
) -> Result<String> {
    let (manifest_path, _) = puller
        .pull_or_get_image(image_ref.as_ref())
        .map_err(|e| anyhow!("failed to pull image: {e}"))?;

    manifest_path
        .file_name()
        .map(|str| str.to_str().unwrap_or("unknown").to_string())
        .with_context(|| anyhow!("failed to get manifest hash"))
}

#[allow(unused)]
pub fn get_bundle_from_image_ref(
    puller: &impl ImagePuller,
    image_ref: impl AsRef<str>,
) -> Result<PathBuf> {
    let manifest_hash = get_manifest_from_image_ref(puller, image_ref)?;
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
fn generate_unique_bundle_path() -> String {
    let container_hash = uuid::Uuid::new_v4().to_string();
    format!("{RKL_BUNDLE_STORE}/{container_hash}")
}

/// pull image using the provided puller
pub fn handle_oci_image(
    puller: &impl ImagePuller,
    image_ref: impl AsRef<str>,
    _name: String,
) -> Result<(ImageConfiguration, String)> {
    let (manifest_path, layers) = puller
        .pull_or_get_image(image_ref.as_ref())
        .map_err(|e| anyhow!("failed to pull image: {e}"))?;

    debug!("get manifest_path: {manifest_path:?}");
    debug!("layers: {layers:?}");

    let bundle_path = generate_unique_bundle_path();

    let config = get_image_config(&manifest_path)?;

    if PathBuf::from(&bundle_path).exists() {
        return Ok((config, "".to_string()));
    }

    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(bundle::mount_and_copy_bundle(bundle_path.clone(), &layers))?;
    Ok((config, bundle_path))
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

// Helper to handle image type and pulling
pub fn handle_image_typ(
    puller: &impl ImagePuller,
    container_spec: &ContainerSpec,
) -> Result<(Option<ContainerConfigBuilder>, String)> {
    if let ImageType::OCIImage = determine_image(&container_spec.image)? {
        let (image_config, bundle_path) =
            handle_oci_image(puller, &container_spec.image, container_spec.name.clone())?;
        // handle image_config
        let mut builder = ContainerConfigBuilder::default();
        if let Some(config) = image_config.config() {
            // add cmd to config
            builder.args_from_image_config(config.entrypoint(), config.cmd());
            // extend env
            builder.envs_from_image_config(config.env());
            // set work_dir
            builder.work_dir(config.working_dir());
            // builder.users(config.user());
        }
        return Ok((Some(builder), bundle_path));
    }
    Ok((None, "".to_string()))
}
