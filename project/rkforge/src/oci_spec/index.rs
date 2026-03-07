use anyhow::{Context, Result};
use oci_spec::image::{
    DescriptorBuilder, ImageIndex, ImageIndexBuilder, MediaType, SCHEMA_VERSION, Sha256Digest,
};
use std::{collections::HashMap, str::FromStr};

pub struct OciImageIndex {
    pub image_index_builder: ImageIndexBuilder,
    pub reference_names: Vec<String>,
    pub annotations: HashMap<String, String>,
    pub descriptor_annotations: HashMap<String, String>,
}

impl OciImageIndex {
    pub fn reference_names(mut self, reference_names: Vec<String>) -> Self {
        self.reference_names = if reference_names.is_empty() {
            vec!["latest".to_string()]
        } else {
            reference_names
        };
        self
    }

    pub fn annotations(mut self, annotations: HashMap<String, String>) -> Self {
        self.annotations = annotations;
        self
    }

    pub fn descriptor_annotations(
        mut self,
        descriptor_annotations: HashMap<String, String>,
    ) -> Self {
        self.descriptor_annotations = descriptor_annotations;
        self
    }

    pub fn manifests(mut self, manifests: Vec<(u64, String)>) -> Result<Self> {
        let mut descriptors = Vec::new();
        let reference_names = if self.reference_names.is_empty() {
            vec!["latest".to_string()]
        } else {
            self.reference_names.clone()
        };

        for (size, digest_str) in manifests.iter() {
            for ref_name in &reference_names {
                let mut annotations = self.descriptor_annotations.clone();
                annotations.insert(
                    String::from("org.opencontainers.image.ref.name"),
                    ref_name.to_string(),
                );

                let descriptor = DescriptorBuilder::default()
                    .media_type(MediaType::ImageManifest)
                    .size(*size)
                    .digest(
                        Sha256Digest::from_str(digest_str.as_str())
                            .with_context(|| format!("Invalid digest format: {digest_str}"))?,
                    )
                    .annotations(annotations)
                    .build()?;

                descriptors.push(descriptor);
            }
        }

        self.image_index_builder = self.image_index_builder.manifests(descriptors);

        Ok(self)
    }

    pub fn build(mut self) -> Result<ImageIndex> {
        if !self.annotations.is_empty() {
            self.image_index_builder = self.image_index_builder.annotations(self.annotations);
        }
        Ok(self.image_index_builder.build()?)
    }
}

impl Default for OciImageIndex {
    fn default() -> Self {
        let image_index_builder = ImageIndexBuilder::default()
            .schema_version(SCHEMA_VERSION)
            .media_type(MediaType::ImageIndex);
        OciImageIndex {
            image_index_builder,
            reference_names: vec!["latest".to_string()],
            annotations: HashMap::new(),
            descriptor_annotations: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::OciImageIndex;

    #[test]
    fn test_multiple_reference_names() {
        let digest = "a".repeat(64);
        let image_index = OciImageIndex::default()
            .reference_names(vec!["v1".to_string(), "latest".to_string()])
            .manifests(vec![(123, digest)])
            .unwrap()
            .build()
            .unwrap();

        assert_eq!(image_index.manifests().len(), 2);
        let mut refs = image_index
            .manifests()
            .iter()
            .map(|descriptor| {
                descriptor
                    .annotations()
                    .as_ref()
                    .and_then(|ann| ann.get("org.opencontainers.image.ref.name"))
                    .cloned()
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>();
        refs.sort();
        assert_eq!(refs, vec!["latest".to_string(), "v1".to_string()]);
    }

    #[test]
    fn test_annotations() {
        let digest = "b".repeat(64);
        let image_index = OciImageIndex::default()
            .annotations(HashMap::from([(
                "org.opencontainers.image.source".to_string(),
                "https://example.com/repo".to_string(),
            )]))
            .descriptor_annotations(HashMap::from([(
                "org.opencontainers.image.revision".to_string(),
                "deadbeef".to_string(),
            )]))
            .manifests(vec![(123, digest)])
            .unwrap()
            .build()
            .unwrap();

        assert_eq!(
            image_index
                .annotations()
                .as_ref()
                .and_then(|a| a.get("org.opencontainers.image.source")),
            Some(&"https://example.com/repo".to_string())
        );

        let descriptor_annotations = image_index.manifests()[0].annotations().as_ref().unwrap();
        assert_eq!(
            descriptor_annotations.get("org.opencontainers.image.revision"),
            Some(&"deadbeef".to_string())
        );
        assert_eq!(
            descriptor_annotations.get("org.opencontainers.image.ref.name"),
            Some(&"latest".to_string())
        );
    }
}
