use std::{collections::HashMap, str::FromStr};

use anyhow::{Context, Result};
use oci_spec::image::{DescriptorBuilder, ImageIndex, ImageIndexBuilder, MediaType, Sha256Digest, SCHEMA_VERSION};

pub struct OciImageIndex {
    pub image_index_builder: ImageIndexBuilder,
}

impl OciImageIndex {
    pub fn manifests(mut self, manifests: Vec<(u64, String)>) -> Result<Self> {
        let mut descriptors = Vec::new();

        for (size, digest_str) in manifests.iter() {
            let descriptor = DescriptorBuilder::default()
                .media_type(MediaType::ImageManifest)
                .size(*size)
                .digest(Sha256Digest::from_str(digest_str.as_str()).with_context(|| {
                    format!("Invalid digest format: {}", digest_str)
                })?)
                .annotations(
                    vec![
                        (String::from("org.opencontainers.image.ref.name"), String::from("latest")),
                    ]
                    .into_iter()
                    .collect::<HashMap<_, _>>(),
                )
                .build()
                .with_context(|| format!("Failed to build manifest descriptor {}", digest_str))?;

            descriptors.push(descriptor);
        }

        self.image_index_builder = self.image_index_builder
            .manifests(descriptors);

        Ok(self)
    }

    pub fn build(self) -> Result<ImageIndex> {
        self.image_index_builder
            .build()
            .with_context(|| "Failed to build image index")
    }
}

impl Default for OciImageIndex {
    fn default() -> Self {
        let image_index_builder = ImageIndexBuilder::default()
            .schema_version(SCHEMA_VERSION)
            .media_type(MediaType::ImageIndex);
        OciImageIndex {
            image_index_builder,
        }
    }
}