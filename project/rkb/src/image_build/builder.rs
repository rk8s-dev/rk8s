use super::executor::Executor;
use crate::{
    compressor::tar_gz_compressor::TarGzCompressor,
    oci_spec::{
        builder::OCIBuilder, config::OciImageConfig, index::OciImageIndex,
        manifest::OciImageManifest,
    },
    overlayfs::MountConfig,
};
use anyhow::{Context, Result};
use dockerfile_parser::Dockerfile;
use std::{collections::HashMap, path::PathBuf};

pub struct Builder {
    pub dockerfile: Dockerfile,
    pub mount_config: MountConfig,
    pub image_output_dir: PathBuf,
}

impl Builder {
    pub fn new(dockerfile: Dockerfile) -> Self {
        Self {
            dockerfile,
            mount_config: MountConfig::default(),
            image_output_dir: PathBuf::default(),
        }
    }

    pub fn mount_config(mut self, mount_config: MountConfig) -> Self {
        self.mount_config = mount_config;
        self
    }

    pub fn image_output_dir(mut self, image_output_dir: PathBuf) -> Self {
        self.image_output_dir = image_output_dir;
        self
    }

    pub fn parse_global_args(&self) -> HashMap<String, Option<String>> {
        let mut global_args = HashMap::new();
        for global_arg in self.dockerfile.global_args.iter() {
            let key = global_arg.name.content.clone();
            let value = global_arg.value.as_ref().map(|value| value.content.clone());
            global_args.insert(key, value);
        }
        global_args
    }

    pub fn build_image(&self) -> Result<()> {
        let global_args = self.parse_global_args();
        let mut executor = Executor::new(self.image_output_dir.clone(), Box::new(TarGzCompressor))
            .stage_executor_config(&global_args);
        executor.execute_stages(&self.dockerfile.stages().stages)?;

        // By now, the layers should be in `image_output_dir/blobs/sha256`
        // construct the image metadata

        // TODO: Add `executor.image_config` to OciImageConfig
        let config = executor
            .image_config
            .get_oci_image_config()
            .context("Failed to get OCI image config")?;
        let image_config = OciImageConfig::default()
            .config(config)
            .and_then(|config| {
                let layer_ids: Vec<String> = executor
                    .image_layers
                    .iter()
                    .map(|l| l.tar_sha256sum.clone())
                    .collect();
                config.rootfs(layer_ids)
            })?;

        let image_manifest = OciImageManifest::default().layers(
            executor
                .image_layers
                .iter()
                .map(|l| (l.gz_size, l.gz_sha256sum.clone()))
                .collect::<Vec<(u64, String)>>(),
        )?;

        let image_index = OciImageIndex::default();

        let oci_builder = OCIBuilder::default()
            .image_dir(self.image_output_dir.clone())
            .oci_image_config(image_config)
            .oci_image_manifest(image_manifest)
            .oci_image_index(image_index);

        oci_builder
            .build()
            .context("Failed to build OCI metadata")?;

        Ok(())
    }
}
