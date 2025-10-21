use anyhow::{Context, Result};
use oci_spec::image::{
    DescriptorBuilder, ImageManifest, ImageManifestBuilder, MediaType, SCHEMA_VERSION, Sha256Digest,
};
use std::str::FromStr;

pub struct OciImageManifest {
    pub image_manifest_builder: ImageManifestBuilder,
}

impl OciImageManifest {
    pub fn config(mut self, config_sha256sum: String, config_size: u64) -> Result<Self> {
        let config = DescriptorBuilder::default()
            .media_type(MediaType::ImageConfig)
            .size(config_size)
            .digest(
                Sha256Digest::from_str(config_sha256sum.as_str())
                    .with_context(|| format!("Invalid digest format: {config_sha256sum}"))?,
            )
            .build()?;

        self.image_manifest_builder = self.image_manifest_builder.config(config);

        Ok(self)
    }

    pub fn layers(mut self, layers: Vec<(u64, String)>) -> Result<Self> {
        let mut descriptors = Vec::new();

        for (size, digest_str) in layers.iter() {
            let digest = Sha256Digest::from_str(digest_str.as_str())
                .with_context(|| format!("Invalid digest format: {digest_str}"))?;

            let descriptor = DescriptorBuilder::default()
                .media_type(MediaType::ImageLayerGzip)
                .size(*size)
                .digest(digest)
                .build()?;

            descriptors.push(descriptor);
        }

        self.image_manifest_builder = self.image_manifest_builder.layers(descriptors);

        Ok(self)
    }

    pub fn build(self) -> Result<ImageManifest> {
        Ok(self.image_manifest_builder.build()?)
    }
}

impl Default for OciImageManifest {
    fn default() -> Self {
        let image_manifest_builder = ImageManifestBuilder::default()
            .schema_version(SCHEMA_VERSION)
            .media_type(MediaType::ImageManifest);
        OciImageManifest {
            image_manifest_builder,
        }
    }
}
