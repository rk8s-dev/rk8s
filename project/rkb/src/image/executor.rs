use crate::{
    compressor::{LayerCompressionConfig, LayerCompressionResult, LayerCompressor},
    image::{BLOBS, config::ImageConfig, context::StageContext, stage_executor::StageExecutor},
    oci_spec::{
        builder::OciBuilder, config::OciImageConfig, index::OciImageIndex,
        manifest::OciImageManifest,
    },
    overlayfs::{MountConfig, OverlayGuard},
};
use anyhow::{Context, Result};
use dockerfile_parser::Dockerfile;
use rayon::prelude::*;
use std::fs;
use std::sync::Arc;
use std::{collections::HashMap, path::PathBuf};

/// Executor coordinates the entire build by using one or more
/// StageExecutors to handle each stage of the build.
///
/// [Reference](https://github.com/containers/buildah/blob/main/imagebuildah/executor.go)
pub struct Executor {
    // Use a guard to ensure all build directories are cleaned up
    guard: OverlayGuard,

    pub dockerfile: Dockerfile,
    pub context: PathBuf,
    pub image_output_dir: PathBuf,

    pub mount_config: MountConfig,
    pub image_config: ImageConfig,
    pub image_aliases: HashMap<String, String>,
    pub image_layers: Vec<LayerCompressionResult>,
    pub global_args: HashMap<String, Option<String>>,

    pub compressor: Arc<dyn LayerCompressor + Send + Sync>,
}

impl Executor {
    pub fn new(
        dockerfile: Dockerfile,
        context: PathBuf,
        image_output_dir: PathBuf,
        global_args: HashMap<String, Option<String>>,
        compressor: Arc<dyn LayerCompressor + Send + Sync>,
    ) -> Self {
        let mount_config = MountConfig::default();
        Self {
            guard: OverlayGuard::new(mount_config.overlay.clone()),
            dockerfile,
            context,
            image_output_dir,
            mount_config,
            image_config: ImageConfig::default(),
            image_aliases: HashMap::new(),
            image_layers: Vec::new(),
            global_args,
            compressor,
        }
    }

    pub fn libfuse(&mut self, libfuse: bool) {
        self.mount_config.libfuse = libfuse;
    }

    pub fn build_image(&mut self) -> Result<()> {
        self.execute_stages()?;
        self.compress_layers()?;
        self.generate_oci_metadata()?;

        Ok(())
    }

    fn execute_stages(&mut self) -> Result<()> {
        let stages = self.dockerfile.stages();
        stages.into_iter().try_for_each(|stage| {
            let ctx = StageContext::new(
                &mut self.mount_config,
                &mut self.image_config,
                &mut self.image_aliases,
                HashMap::new(),
                &self.global_args,
                self.context.clone(),
            );
            let mut stage_executor = StageExecutor::new(ctx, stage);
            stage_executor.execute()
        })
    }

    fn compress_layers(&mut self) -> Result<()> {
        // check if `image_output_dir/blobs/sha256` exists
        let layer_dir = self.image_output_dir.join(BLOBS);
        if !layer_dir.exists() {
            fs::create_dir_all(&layer_dir)
                .with_context(|| format!("Failed to create directory {}", layer_dir.display()))?;
        }
        // parallel compression
        let mut compression_result = self
            .mount_config
            .lower_dir
            .par_iter()
            .enumerate()
            .map(|(i, layer)| {
                let compression_config =
                    LayerCompressionConfig::new(layer.clone(), layer_dir.clone());
                let compression_result = self
                    .compressor
                    .compress_layer(&compression_config)
                    .with_context(|| format!("Failed to compress layer {}", layer.display()))?;
                Ok((i, compression_result))
            })
            .collect::<Result<Vec<_>>>()?;
        // add to image layers, sorted by original order
        compression_result.sort_by_key(|x| x.0);
        self.image_layers
            .extend(compression_result.into_iter().map(|x| x.1));
        Ok(())
    }

    fn generate_oci_metadata(&self) -> Result<()> {
        let config = self
            .image_config
            .get_oci_image_config()
            .context("Failed to get OCI image config")?;
        let image_config = OciImageConfig::default()
            .config(config)
            .and_then(|config| {
                let layer_ids: Vec<String> = self
                    .image_layers
                    .iter()
                    .map(|l| l.tar_sha256sum.clone())
                    .collect();
                config.rootfs(layer_ids)
            })?;

        let image_manifest = OciImageManifest::default().layers(
            self.image_layers
                .iter()
                .map(|l| (l.gz_size, l.gz_sha256sum.clone()))
                .collect::<Vec<(u64, String)>>(),
        )?;

        let image_index = OciImageIndex::default();
        let oci_builder = OciBuilder::default()
            .image_dir(self.image_output_dir.clone())
            .oci_image_config(image_config)
            .oci_image_manifest(image_manifest)
            .oci_image_index(image_index);

        oci_builder.build().context("Failed to build OCI metadata")
    }
}
