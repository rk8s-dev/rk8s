mod pusher;

use crate::config::auth::AuthConfig;
use crate::push::pusher::{PushTask, Pusher};
use crate::registry::{
    parse_registry_host, parse_registry_host_arg, resolve_client_ref_auth as resolve_ref_with_auth,
};
use crate::rt::block_on;
use crate::storage::{DigestExt, parse_image_ref};
use anyhow::{Context, bail};
use clap::Parser;
use oci_client::client::ImageLayer;
use oci_client::manifest::{OciImageIndex, OciManifest};
use oci_client::secrets::RegistryAuth;
use oci_client::{Client, client};
use oci_spec::distribution::Reference;
use std::collections::HashMap;
use std::path::Path;
use tokio::io::AsyncReadExt;

macro_rules! from_oci_blob {
    ($output:ty, $path:expr, $descriptor:expr) => {{
        let mut buffer = Vec::new();
        let mut file = tokio::fs::File::open($path).await?;
        file.read_to_end(&mut buffer).await?;

        <$output>::new(
            buffer,
            $descriptor.media_type.clone(),
            $descriptor.annotations.clone(),
        )
    }};
}

#[derive(Parser, Debug)]
pub struct PushArgs {
    /// Image reference
    image_ref: String,
    /// Image path (default current directory)
    #[arg(long)]
    path: Option<String>,
    /// Registry host in `host[:port]` format.
    #[arg(long, value_parser = parse_registry_host_arg)]
    url: Option<String>,
    /// Skip TLS certificate verification for HTTPS registry.
    #[arg(long)]
    skip_tls_verify: bool,
}

pub fn push(args: PushArgs) -> anyhow::Result<()> {
    let path = args.path.unwrap_or(".".to_string());
    push_from_layout_with_tls(args.image_ref, path, args.url, args.skip_tls_verify)
}

pub fn push_from_layout(
    image_ref: impl Into<String>,
    path: impl AsRef<Path>,
    url: Option<String>,
) -> anyhow::Result<()> {
    push_from_layout_with_tls(image_ref, path, url, false)
}

fn push_from_layout_with_tls(
    image_ref: impl Into<String>,
    path: impl AsRef<Path>,
    url: Option<String>,
    skip_tls_verify: bool,
) -> anyhow::Result<()> {
    let image_ref = image_ref.into();
    let path = path.as_ref().to_path_buf();
    let auth_config = AuthConfig::load()?;
    let requested_has_explicit_tag = has_explicit_tag(&image_ref);
    let parsed_input_ref = image_ref
        .parse::<Reference>()
        .with_context(|| format!("invalid image reference for push: {}", image_ref.as_str()))?;
    let requested_repo = parsed_input_ref.repository().to_string();

    let url = match url {
        Some(url) => auth_config.resolve_url(Some(url))?,
        None if has_explicit_registry(&image_ref) => {
            parse_registry_host(parsed_input_ref.registry())?
        }
        None => auth_config.resolve_url(None::<String>)?,
    };

    let normalized_image_ref = if let Some(tag) = parsed_input_ref.tag() {
        format!("{requested_repo}:{}", tag)
    } else {
        requested_repo.clone()
    };
    let (client, image_ref, auth_method) =
        resolve_ref_with_auth(&auth_config, &url, &normalized_image_ref, skip_tls_verify)?;
    let registry_url = image_ref.registry().to_string();

    block_on(async move {
        push_image(
            &client,
            &image_ref,
            &auth_method,
            &path,
            &registry_url,
            &requested_repo,
            requested_has_explicit_tag,
        )
        .await
    })?
}

pub async fn push_image(
    client: &Client,
    image_ref: &Reference,
    auth_method: &RegistryAuth,
    path: impl AsRef<Path>,
    registry_url: impl AsRef<str>,
    requested_repo: impl AsRef<str>,
    requested_has_explicit_tag: bool,
) -> anyhow::Result<()> {
    let dir = path.as_ref();
    let registry_url = registry_url.as_ref();
    let requested_repo = requested_repo.as_ref();

    let image_index_path = dir.join("index.json");
    let image_index = serde_json::from_str::<OciImageIndex>(
        &tokio::fs::read_to_string(&image_index_path)
            .await
            .with_context(|| format!("Failed to read from {}", image_index_path.display()))?,
    )?;

    let dir = dir.join("blobs/sha256");
    let requested_tag = if requested_has_explicit_tag {
        image_ref.tag().map(|tag| tag.to_string())
    } else {
        None
    };

    let mut digest_to_ref_names: HashMap<String, Vec<String>> = HashMap::new();
    for descriptor in &image_index.manifests {
        let digest = descriptor.digest.split_digest()?.to_string();
        let ref_name = descriptor
            .annotations
            .as_ref()
            .and_then(|ann| ann.get("org.opencontainers.image.ref.name"))
            .cloned()
            .unwrap_or_else(|| "latest".to_string());

        let entry = digest_to_ref_names.entry(digest).or_default();
        if !entry.contains(&ref_name) {
            entry.push(ref_name);
        }
    }

    let mut tasks = Vec::new();
    let mut matched_requested_tag = false;
    for (digest, ref_names) in digest_to_ref_names {
        if !should_include_digest(requested_tag.as_deref(), &ref_names) {
            continue;
        }
        if requested_tag.is_some() {
            matched_requested_tag = true;
        }

        let manifest_path = dir.join(&digest);
        let manifest = serde_json::from_str::<OciManifest>(
            &tokio::fs::read_to_string(&manifest_path)
                .await
                .with_context(|| format!("Failed to read from {}", manifest_path.display()))?,
        )?;
        let manifest = match manifest {
            OciManifest::Image(manifest) => manifest,
            OciManifest::ImageIndex(_) => anyhow::bail!("Image indexes are not supported yet"),
        };

        let target_refs = if requested_tag.is_some() {
            vec![image_ref.clone()]
        } else {
            ref_names
                .into_iter()
                .map(|ref_name| {
                    parse_image_ref(registry_url, requested_repo, Some(ref_name.as_str()))
                })
                .collect::<anyhow::Result<Vec<_>>>()?
        };

        for target_ref in target_refs {
            let descriptors = &manifest.layers;
            let mut layers = Vec::new();

            for descriptor in descriptors {
                let layer_path = dir.join(descriptor.digest.split_digest()?);
                let layer = from_oci_blob!(ImageLayer, layer_path, descriptor);
                layers.push(layer);
            }

            let config_path = dir.join(manifest.config.digest.split_digest()?);
            let config = from_oci_blob!(client::Config, config_path, manifest.config);

            let auth_method = auth_method.clone();
            let client = client.clone();
            let digest_with_ref = format!("{digest}@{}", target_ref.whole());
            let manifest = manifest.clone();
            let task = PushTask::new(
                digest_with_ref,
                Box::pin(async move {
                    client
                        .push(&target_ref, &layers, config, &auth_method, Some(manifest))
                        .await
                }),
            );
            tasks.push(task);
        }
    }

    if let Some(tag) = requested_tag
        && !matched_requested_tag
    {
        bail!("tag `{tag}` not found in index.json");
    }

    let pusher = Pusher::new(tasks);
    pusher.push_all().await?;
    Ok(())
}

fn should_include_digest(requested_tag: Option<&str>, ref_names: &[String]) -> bool {
    match requested_tag {
        Some(tag) => ref_names.iter().any(|ref_name| ref_name == tag),
        None => true,
    }
}

fn has_explicit_tag(raw: &str) -> bool {
    let raw_without_digest = raw.split_once('@').map(|(name, _)| name).unwrap_or(raw);
    let last_colon = raw_without_digest.rfind(':');
    let last_slash = raw_without_digest.rfind('/');
    match (last_colon, last_slash) {
        (Some(colon), Some(slash)) => colon > slash,
        (Some(_), None) => true,
        _ => false,
    }
}

fn has_explicit_registry(raw: &str) -> bool {
    let raw_without_digest = raw.split_once('@').map(|(name, _)| name).unwrap_or(raw);
    let raw_without_tag = if has_explicit_tag(raw_without_digest) {
        match raw_without_digest.rfind(':') {
            Some(idx) => &raw_without_digest[..idx],
            None => raw_without_digest,
        }
    } else {
        raw_without_digest
    };

    let Some((first, _rest)) = raw_without_tag.split_once('/') else {
        return false;
    };

    first == "localhost" || first.contains('.') || first.contains(':')
}

fn strip_explicit_tag(raw: &str) -> &str {
    let raw_without_digest = raw.split_once('@').map(|(name, _)| name).unwrap_or(raw);
    if has_explicit_tag(raw)
        && let Some(idx) = raw_without_digest.rfind(':')
    {
        return &raw_without_digest[..idx];
    }
    raw_without_digest
}

#[cfg(test)]
mod tests {
    use crate::storage::parse_image_ref;

    use super::{
        has_explicit_registry, has_explicit_tag, should_include_digest, strip_explicit_tag,
    };

    #[test]
    fn test_should_include_digest_for_explicit_tag() {
        let refs = vec!["latest".to_string(), "v1".to_string()];
        assert!(should_include_digest(Some("v1"), &refs));
        assert!(!should_include_digest(Some("v2"), &refs));
    }

    #[test]
    fn test_should_include_digest_without_explicit_tag() {
        let refs = vec!["latest".to_string()];
        assert!(should_include_digest(None, &refs));
    }

    #[test]
    fn test_has_explicit_tag() {
        assert!(!has_explicit_tag("repo/app"));
        assert!(!has_explicit_tag("localhost:5000/repo/app"));
        assert!(has_explicit_tag("repo/app:v1"));
        assert!(has_explicit_tag("localhost:5000/repo/app:v1"));
        assert!(!has_explicit_tag("repo/app@sha256:1234"));
        assert!(has_explicit_tag("repo/app:v1@sha256:1234"));
    }

    #[test]
    fn test_has_explicit_registry() {
        assert!(!has_explicit_registry("repo/app"));
        assert!(has_explicit_registry("my.ns/team/app"));
        assert!(!has_explicit_registry("repo/app:v1"));
        assert!(has_explicit_registry("ghcr.io/acme/app"));
        assert!(has_explicit_registry("ghcr.io/acme/app:v1"));
        assert!(has_explicit_registry("localhost:5000/acme/app:v1"));
        assert!(has_explicit_registry("ghcr.io/acme/app@sha256:1234"));
    }

    #[test]
    fn test_strip_explicit_tag() {
        assert_eq!(strip_explicit_tag("repo/app"), "repo/app");
        assert_eq!(strip_explicit_tag("my.ns/team/app"), "my.ns/team/app");
        assert_eq!(
            strip_explicit_tag("localhost:5000/repo/app"),
            "localhost:5000/repo/app"
        );
        assert_eq!(strip_explicit_tag("repo/app:v1"), "repo/app");
        assert_eq!(
            strip_explicit_tag("localhost:5000/repo/app:v1"),
            "localhost:5000/repo/app"
        );
        assert_eq!(strip_explicit_tag("repo/app@sha256:1234"), "repo/app");
        assert_eq!(strip_explicit_tag("repo/app:v1@sha256:1234"), "repo/app");
    }

    #[test]
    fn test_implicit_push_repo_path_preserved() {
        let ref_name = strip_explicit_tag("my.ns/team/app");
        let target = parse_image_ref("127.0.0.1:8968", ref_name, Some("latest")).unwrap();
        assert_eq!(target.repository(), "my.ns/team/app");
    }

    #[test]
    fn test_registry_qualified_push_ref_is_not_prefixed_twice() {
        let parsed = "ghcr.io/acme/app:v1"
            .parse::<oci_spec::distribution::Reference>()
            .unwrap();
        let normalized = format!("{}:{}", parsed.repository(), parsed.tag().unwrap());
        let target = parse_image_ref(parsed.registry(), normalized, None::<String>).unwrap();
        assert_eq!(target.whole(), "ghcr.io/acme/app:v1");
    }
}
