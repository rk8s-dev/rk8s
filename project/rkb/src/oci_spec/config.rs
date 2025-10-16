use crate::image::config::DEFAULT_ENV;
use anyhow::Result;
use oci_spec::image::{
    Arch, Config, ConfigBuilder, ImageConfiguration, ImageConfigurationBuilder, Os, RootFsBuilder,
};

#[derive(Default)]
pub struct OciImageConfig {
    pub image_config_builder: ImageConfigurationBuilder,
}

impl OciImageConfig {
    pub fn default_config(mut self) -> Result<Self> {
        let config = ConfigBuilder::default()
            .cmd(vec!["sh".to_string()])
            .env(vec![format!("PATH={DEFAULT_ENV}")])
            .build()?;

        self.image_config_builder = self
            .image_config_builder
            .config(config)
            .architecture(Arch::Amd64)
            .os(Os::Linux)
            .created(chrono::Utc::now().to_rfc3339());

        Ok(self)
    }

    pub fn config(mut self, config: Config) -> Result<Self> {
        self.image_config_builder = self
            .image_config_builder
            .config(config)
            .architecture(Arch::Amd64)
            .os(Os::Linux)
            .created(chrono::Utc::now().to_rfc3339());

        Ok(self)
    }

    pub fn rootfs(mut self, rootfs: Vec<String>) -> Result<Self> {
        let rootfs = RootFsBuilder::default()
            .typ("layers".to_string())
            .diff_ids(
                rootfs
                    .iter()
                    .map(|s| format!("sha256:{s}"))
                    .collect::<Vec<String>>(),
            )
            .build()?;

        self.image_config_builder = self.image_config_builder.rootfs(rootfs);

        Ok(self)
    }

    pub fn build(self) -> Result<ImageConfiguration> {
        Ok(self.image_config_builder.build()?)
    }
}
