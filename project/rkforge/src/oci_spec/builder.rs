use std::{fs, path::PathBuf};

use anyhow::Result;
use oci_spec::image::OciLayoutBuilder;

use crate::utils::hash::calculate_sha256;

use super::{config::OciImageConfig, index::OciImageIndex, manifest::OciImageManifest};

/// Currently only supports single manifest
///
/// To support multiple manifests, consider modifying the struct members
/// to be a vector of manifests and update the methods accordingly.
#[derive(Default)]
pub struct OciBuilder {
    pub image_dir: PathBuf,
    pub oci_image_config: OciImageConfig,
    pub oci_image_manifest: OciImageManifest,
    pub oci_image_index: OciImageIndex,
}

impl OciBuilder {
    pub fn image_dir(mut self, image_dir: PathBuf) -> Self {
        self.image_dir = image_dir;
        self
    }

    pub fn oci_image_config(mut self, oci_image_config: OciImageConfig) -> Self {
        self.oci_image_config = oci_image_config;
        self
    }

    pub fn oci_image_manifest(mut self, oci_image_manifest: OciImageManifest) -> Self {
        self.oci_image_manifest = oci_image_manifest;
        self
    }

    pub fn oci_image_index(mut self, oci_image_index: OciImageIndex) -> Self {
        self.oci_image_index = oci_image_index;
        self
    }

    pub fn build(mut self) -> Result<()> {
        let layer_dir = self.image_dir.join("blobs/sha256");

        tracing::info!("Generating OCI image layout...");
        let image_config = self.oci_image_config.build()?;
        let image_config_path = layer_dir.join("config.json");
        image_config.to_file_pretty(&image_config_path)?;

        let image_config_sha256sum = calculate_sha256(&image_config_path)?;
        let new_image_config_path = layer_dir.join(&image_config_sha256sum);
        fs::rename(&image_config_path, &new_image_config_path)?;

        tracing::info!("Generating OCI image manifest...");
        let image_config_metadata = fs::metadata(&new_image_config_path)?;
        self.oci_image_manifest = self
            .oci_image_manifest
            .config(image_config_sha256sum, image_config_metadata.len())?;
        let image_manifest = self.oci_image_manifest.build()?;
        let image_manifest_path = layer_dir.join("manifest.json");
        image_manifest.to_file_pretty(&image_manifest_path)?;
        let image_manifest_sha256sum = calculate_sha256(&image_manifest_path)?;
        let new_image_manifest_path = layer_dir.join(&image_manifest_sha256sum);
        fs::rename(&image_manifest_path, &new_image_manifest_path)?;
        let image_manifest_metadata = fs::metadata(&new_image_manifest_path)?;

        tracing::info!("Generating OCI image index...");
        self.oci_image_index = self.oci_image_index.manifests(vec![(
            image_manifest_metadata.len(),
            image_manifest_sha256sum,
        )])?;
        let image_index = self.oci_image_index.build()?;
        let image_index_path = self.image_dir.join("index.json");
        image_index.to_file_pretty(&image_index_path)?;

        tracing::info!("Generating OCI layout...");
        let oci_layout = OciLayoutBuilder::default()
            .image_layout_version("1.0.0".to_string())
            .build()?;
        let oci_layout_path = self.image_dir.join("oci-layout");
        Ok(oci_layout.to_file_pretty(&oci_layout_path)?)
    }
}
