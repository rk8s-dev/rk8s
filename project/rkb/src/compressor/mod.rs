pub mod tar_gz_compressor;

use std::path::PathBuf;

use anyhow::Result;

#[derive(Debug, Clone)]
pub struct LayerCompressionResult {
    pub tar_sha256sum: String,
    pub tar_size: u64,
    pub gz_sha256sum: String,
    pub gz_size: u64,
}

impl LayerCompressionResult {
    pub fn new(tar_sha256sum: String, tar_size: u64, gz_sha256sum: String, gz_size: u64) -> Self {
        Self {
            tar_sha256sum,
            tar_size,
            gz_sha256sum,
            gz_size,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LayerCompressionConfig {
    pub layer_dir: PathBuf,
    pub output_dir: PathBuf,
}

impl LayerCompressionConfig {
    pub fn new(layer_dir: PathBuf, output_dir: PathBuf) -> Self {
        Self {
            layer_dir,
            output_dir,
        }
    }
}

pub trait LayerCompressor {
    /// Compress layer to tar.gz
    ///
    /// Returns the size and sha256sum of the result
    fn compress_layer(&self, config: &LayerCompressionConfig) -> Result<LayerCompressionResult>;
}
