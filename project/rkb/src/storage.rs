use crate::config::image::CONFIG;
use crate::config::meta::Repositories;
use anyhow::Context;
use oci_client::manifest::OciManifest;
use oci_spec::distribution::Reference;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

pub async fn write_manifest(
    image_ref: &Reference,
    manifest: &OciManifest,
    digest: impl AsRef<str>,
) -> anyhow::Result<PathBuf> {
    let digest = digest.as_ref();

    let path = ultimate_blob_path(digest)?;
    write_to(&path, &manifest).await?;

    let image_ref = full_image_ref(image_ref.repository(), image_ref.tag());
    Repositories::add_store(vec![(image_ref, digest)])?;
    Ok(path)
}

pub async fn write_to<T: Serialize>(dst: impl AsRef<Path>, object: &T) -> anyhow::Result<()> {
    let dst = dst.as_ref();
    let mut file = tokio::fs::File::create(dst)
        .await
        .with_context(|| format!("Failed to create file {}", dst.display()))?;
    file.write_all(serde_json::to_string_pretty(&object)?.as_bytes())
        .await
        .with_context(|| format!("Failed to write to file {}", dst.display()))?;
    Ok(())
}

pub fn parse_image_ref(
    url: impl AsRef<str>,
    image_ref: impl AsRef<str>,
    tag: Option<impl AsRef<str>>,
) -> anyhow::Result<Reference> {
    let url = url.as_ref();
    let image_ref = image_ref.as_ref();

    format!("{url}/{}", full_image_ref(image_ref, tag))
        .parse::<Reference>()
        .with_context(|| format!("could not parse image reference: {}", url))
}

/// Constructs a full image reference by joining a base reference and a new tag. If the base reference does not contain namespace,
/// it will be expanded to `library/{}`.
///
/// This function performs a simple string concatenation. It does not attempt to parse the `image_ref` or replace an existing tag.
///
/// # Parameters
/// - `image_ref`: The base image reference string, with an optional tag.
///   For example: `ubuntu` or `ubuntu:latest`.
/// - `tag`: An `Option` containing the tag string to append. If `None`, the `image_ref` is returned unchanged.
///
/// # Errors
/// This function does not handle complex logic. It will produce incorrect results if the `image_ref` already contains a tag
/// and a new tag is provided. The new tag will simply be appended after another colon.
///
/// # Examples
///
/// ```rust
/// assert_eq(full_image_ref("ubuntu", Some("latest")), "library/ubuntu:latest"));
/// assert_eq(full_image_ref("ubuntu:latest", Some("latest"), "library/ubuntu:latest:latest"))
/// ```
pub fn full_image_ref(image_ref: impl AsRef<str>, tag: Option<impl AsRef<str>>) -> String {
    let image_ref = image_ref.as_ref();

    let image_ref = if image_ref.contains("/") {
        image_ref.to_string()
    } else {
        format!("library/{image_ref}")
    };

    format!(
        "{image_ref}{}",
        match tag {
            Some(tag) => format!(":{}", tag.as_ref()),
            None => "".to_string(),
        }
    )
}

pub trait DigestExt {
    fn split_digest(&self) -> anyhow::Result<&str>;
}

impl DigestExt for String {
    fn split_digest(&self) -> anyhow::Result<&str> {
        let (_, digest) = self
            .rsplit_once(":")
            .with_context(|| format!("invalid digest format: {}", self))?;
        Ok(digest)
    }
}

impl DigestExt for &str {
    fn split_digest(&self) -> anyhow::Result<&str> {
        let (_, digest) = self
            .rsplit_once(":")
            .with_context(|| format!("invalid digest format: {}", self))?;
        Ok(digest)
    }
}

/// Get the ultimate path of a blob in the layer storage directory.
///
/// # Note
/// You will get an error if you don't use a full digest string with algorithm.
///
/// ## correct
/// `sha256:abcdef...`
///
/// ## incorrect
///
/// `abcdef...`
pub fn ultimate_blob_path(digest: impl AsRef<str>) -> anyhow::Result<PathBuf> {
    let digest = digest.as_ref();
    Ok(CONFIG.layers_store_root.join(digest.split_digest()?))
}
