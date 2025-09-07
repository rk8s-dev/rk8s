use super::{config::ImageConfig, config::StageExecutorConfig, stage_executor::StageExecutor};
use crate::compressor::LayerCompressor;
use crate::{
    compressor::{LayerCompressionConfig, LayerCompressionResult},
    config,
    overlayfs::{MountConfig, OverlayGuard},
};
use anyhow::{Context, Result};
use dockerfile_parser::Stage;
use std::fs;
use std::{collections::HashMap, path::PathBuf};

/// Executor coordinates the entire build by using one or more
/// StageExecutors to handle each stage of the build.
///
/// [Reference](https://github.com/containers/buildah/blob/main/imagebuildah/executor.go)
pub struct Executor {
    pub mount_config: MountConfig,
    pub stage_executor_config: StageExecutorConfig,
    pub image_config: ImageConfig,
    pub image_aliases: HashMap<String, String>,
    pub image_output_dir: PathBuf,
    pub image_layers: Vec<LayerCompressionResult>,
    pub compressor: Box<dyn LayerCompressor>,
}

impl Executor {
    pub fn new(image_output_dir: PathBuf, compressor: Box<dyn LayerCompressor>) -> Self {
        Self {
            mount_config: MountConfig::default(),
            stage_executor_config: StageExecutorConfig::default(),
            image_config: ImageConfig::default(),
            image_aliases: HashMap::new(),
            image_output_dir,
            image_layers: Vec::new(),
            compressor,
        }
    }

    pub fn stage_executor_config(mut self, global_args: &HashMap<String, Option<String>>) -> Self {
        self.stage_executor_config = StageExecutorConfig::default().global_args(global_args);
        self
    }

    pub fn execute_stages(&mut self, stages: &Vec<Stage<'_>>) -> Result<()> {
        let _guard = OverlayGuard::new(self.mount_config.overlay.clone());
        for stage in stages.iter() {
            self.execute_stage(stage)
                .context("Failed to execute stages")?;
        }

        // check if `image_output_dir/blobs/sha256` exists
        let layer_dir = self.image_output_dir.join(config::BLOBS);
        if !layer_dir.exists() {
            fs::create_dir_all(&layer_dir)
                .with_context(|| format!("Failed to create directory {}", layer_dir.display()))?;
        }

        // compress layers
        for layer in self.mount_config.lower_dir.iter() {
            let compression_config = LayerCompressionConfig::new(layer.clone(), layer_dir.clone());
            let compression_result = self
                .compressor
                .compress_layer(&compression_config)
                .with_context(|| format!("Failed to compress layer {}", layer.display()))?;
            self.image_layers.push(compression_result);
        }
        Ok(())
    }

    pub fn execute_stage(&mut self, stage: &Stage<'_>) -> Result<()> {
        let mut stage_executor = StageExecutor::new(
            &mut self.mount_config,
            &mut self.image_config,
            &mut self.image_aliases,
        );

        stage_executor
            .execute(stage, &self.stage_executor_config)
            .with_context(|| format!("Failed to execute stage {stage:?}"))?;
        Ok(())
    }
}
