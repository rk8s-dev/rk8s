use anyhow::{Context, Result};
use oci_spec::image::{
    Arch, Config, ConfigBuilder, ImageConfiguration, ImageConfigurationBuilder, Os, RootFsBuilder,
};

#[allow(dead_code)]
static DEFAULT_ENV: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";

#[derive(Default)]
pub struct OciImageConfig {
    pub image_config_builder: ImageConfigurationBuilder,
}

impl OciImageConfig {
    #[allow(dead_code)]
    pub fn default_config(mut self) -> Result<Self> {
        let config = ConfigBuilder::default()
            .cmd(vec!["sh".to_string()])
            .env(vec![DEFAULT_ENV.to_string()])
            .build()
            .context("Failed to build default config")?;

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
            .build()
            .context("Failed to build rootfs")?;

        self.image_config_builder = self.image_config_builder.rootfs(rootfs);

        Ok(self)
    }

    pub fn build(self) -> Result<ImageConfiguration> {
        self.image_config_builder
            .build()
            .context("Failed to build image configuration")
    }
}
