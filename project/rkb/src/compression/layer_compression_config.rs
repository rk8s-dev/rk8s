use std::path::PathBuf;

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
