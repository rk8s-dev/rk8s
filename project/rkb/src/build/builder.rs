use std::{collections::HashMap, path::PathBuf};

use anyhow::{Context, Result};

use dockerfile_parser::Dockerfile;

use crate::{oci_spec::{oci_builder::OCIBuilder, oci_image_config::OciImageConfig, oci_image_index::OciImageIndex, oci_image_manifest::OciImageManifest}, overlayfs::mount_config::MountConfig};

use super::{build_config::BuildConfig, executor::Executor};

pub struct Builder {
    pub dockerfile: Dockerfile,
    pub mount_config: MountConfig,
    pub image_output_dir: PathBuf,
    pub build_config: BuildConfig,
}

impl Builder {
    pub fn new(dockerfile: Dockerfile) -> Self {
        Self {
            dockerfile,
            mount_config: MountConfig::default(),
            image_output_dir: PathBuf::default(),
            build_config: BuildConfig::default(),
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

    pub fn config(mut self, build_config: BuildConfig) -> Self {
        self.build_config = build_config;
        self
    }

    pub fn parse_global_args(&self) -> HashMap<String, Option<String>> {
        let mut global_args = HashMap::new();
        for global_arg in self.dockerfile.global_args.iter() {
            let key = global_arg.name.content.clone();
            let value = if let Some(value) = &global_arg.value {
                Some(value.content.clone())
            } else {
                None
            };
            global_args.insert(key, value);
        }
        global_args
    }

    pub fn build_image(&self) -> Result<()> {
        let global_args = self.parse_global_args();
        let mut executor = Executor::new(self.image_output_dir.clone())
            .stage_executor_config(self.build_config.clone(), &global_args);
        executor.execute_stages(&self.dockerfile.stages().stages)?;

        // by now, the layers should be in `image_output_dir/blobs/sha256`
        // construct the image metadata

        // TODO: add `executor.image_config` to OciImageConfig
        let config = executor.image_config.get_oci_image_config()
            .with_context(|| format!("Failed to get OCI image config"))?;
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

        let image_manifest = OciImageManifest::default()
            .layers(executor.image_layers
                .iter()
                .map(|l| (l.gz_size, l.gz_sha256sum.clone()))
                .collect::<Vec<(u64, String)>>()
            )?;

        let image_index = OciImageIndex::default();

        let oci_builder = OCIBuilder::default()
            .image_dir(self.image_output_dir.clone())
            .oci_image_config(image_config)
            .oci_image_manifest(image_manifest)
            .oci_image_index(image_index);

        oci_builder.build()
            .with_context(|| format!("Failed to build OCI metadata"))?;

        Ok(())
    }
}