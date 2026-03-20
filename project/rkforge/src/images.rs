use crate::compressor::tar_gz_compressor::TarGzCompressor;
use crate::compressor::{LayerCompressionConfig, LayerCompressor};
use crate::config::meta::Repositories;
use crate::pull::media::{MediaType, get_media_type};
use crate::storage::{DigestExt, full_image_ref, read_manifest, ultimate_blob_path};
use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Local};
use clap::Parser;
use oci_client::manifest::{OciImageManifest, OciManifest};
use serde_json::Value;
use std::collections::HashSet;
use std::io::{self, Write};
use std::path::{Component, Path};
use tabwriter::TabWriter;

// ---------------------------------------------------------------------------
// CLI argument structs
// ---------------------------------------------------------------------------

/// CLI arguments for the `images` command
#[derive(Parser, Debug)]
pub struct ImagesArgs {
    /// Only display image IDs
    #[arg(long, short)]
    pub quiet: bool,
}

/// CLI arguments for the `inspect` command
#[derive(Parser, Debug)]
pub struct InspectArgs {
    /// Image reference (e.g. "ubuntu:latest" or "library/nginx:1.25")
    #[arg(value_name = "IMAGE_REF")]
    pub image_ref: String,
}

/// CLI arguments for the `tag` command
#[derive(Parser, Debug)]
pub struct TagArgs {
    /// Source image reference
    #[arg(value_name = "SOURCE")]
    pub source: String,
    /// Target image reference (new tag)
    #[arg(value_name = "TARGET")]
    pub target: String,
}

/// CLI arguments for the `rmi` command
#[derive(Parser, Debug)]
pub struct RmiArgs {
    /// Image reference or ID to remove
    #[arg(value_name = "IMAGE_REF")]
    pub image_ref: String,
    /// Force removal even if errors occur during layer cleanup
    #[arg(long, short)]
    pub force: bool,
}

/// CLI arguments for the `save` command
#[derive(Parser, Debug)]
pub struct SaveArgs {
    /// Image reference to export
    #[arg(value_name = "IMAGE_REF")]
    pub image_ref: String,
    /// Output file path (e.g. "image.tar")
    #[arg(long, short)]
    pub output: String,
}

/// CLI arguments for the `load` command
#[derive(Parser, Debug)]
pub struct LoadArgs {
    /// Input tar file path
    #[arg(long, short)]
    pub input: String,
    /// Optional image reference to assign (overrides annotation in archive)
    #[arg(long)]
    pub tag: Option<String>,
}

// ---------------------------------------------------------------------------
// images (ls)
// ---------------------------------------------------------------------------

/// List all locally cached images
pub fn list_images(args: ImagesArgs) -> Result<()> {
    let repos = Repositories::load()?;
    let entries = repos.entries();

    if entries.is_empty() {
        println!("No images found.");
        return Ok(());
    }

    if args.quiet {
        for (_image_ref, digest) in &entries {
            println!("{}", short_id(digest));
        }
        return Ok(());
    }

    let mut tab_writer = TabWriter::new(io::stdout());
    writeln!(&mut tab_writer, "REPOSITORY\tTAG\tIMAGE ID\tSIZE\tCREATED")?;

    for (image_ref, digest) in &entries {
        let (repository, tag) = split_image_ref(image_ref);
        let image_id = short_id(digest);
        let (size, created) =
            get_image_metadata(digest).unwrap_or_else(|_| ("N/A".to_string(), "N/A".to_string()));

        writeln!(
            &mut tab_writer,
            "{}\t{}\t{}\t{}\t{}",
            repository, tag, image_id, size, created
        )?;
    }

    tab_writer.flush()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// inspect
// ---------------------------------------------------------------------------

/// Display detailed information about an image in JSON format
pub fn inspect_image(args: InspectArgs) -> Result<()> {
    let digest = resolve_image_digest(&args.image_ref)?;
    let manifest = read_manifest(&digest)?;

    match &manifest {
        OciManifest::Image(img) => {
            let config = read_image_config_value(&img.config.digest)?.unwrap_or(Value::Null);
            let output = serde_json::json!({
                "digest": digest,
                "manifest": img,
                "config": config,
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        OciManifest::ImageIndex(idx) => {
            let output = serde_json::json!({
                "digest": digest,
                "index": idx,
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// tag
// ---------------------------------------------------------------------------

/// Create a new tag (alias) for an existing image
pub fn tag_image(args: TagArgs) -> Result<()> {
    let source_ref = normalize_image_ref(&args.source);
    let target_ref = normalize_image_ref(&args.target);

    let repos = Repositories::load()?;
    let digest = repos
        .get(&source_ref)?
        .ok_or_else(|| anyhow!("Image '{}' not found locally", args.source))?
        .to_string();

    Repositories::add_store(vec![(target_ref.clone(), digest)])?;
    println!("Tagged {} as {}", args.source, target_ref);
    Ok(())
}

// ---------------------------------------------------------------------------
// rmi
// ---------------------------------------------------------------------------

/// Remove an image from local storage.
///
/// Ordering: the tag is removed from the in-memory `Repositories` first, then
/// unreferenced blobs are deleted, and finally the updated metadata is persisted.
/// If blob cleanup fails and `--force` is not set, the tag is restored and
/// re-persisted so the state remains consistent. However, if blob cleanup
/// succeeds but the final `repos.store()` fails, the blobs are already gone
/// while `repositories.toml` still references them — a known edge case that
/// would require transactional storage to fully eliminate.
pub fn remove_image(args: RmiArgs) -> Result<()> {
    let mut repos = Repositories::load()?;
    let image_ref = resolve_remove_target(&args.image_ref, &repos)?;
    let digest = repos
        .remove(&image_ref)
        .ok_or_else(|| anyhow!("Image '{}' not found locally", args.image_ref))?;

    let still_referenced = repos.digests().iter().any(|d| **d == digest);

    if !still_referenced && let Err(e) = cleanup_image_blobs(&digest, &repos) {
        if args.force {
            eprintln!("Warning: blob cleanup failed: {e}");
        } else {
            repos.add(&image_ref, &digest);
            repos.store()?;
            return Err(e.context("Failed to clean up blobs; tag restored"));
        }
    }

    repos.store()?;
    println!("Untagged: {image_ref}");
    if !still_referenced {
        println!("Deleted: {}", short_id(&digest));
    }
    Ok(())
}

/// Remove unreferenced blobs for a given manifest digest.
///
/// Blobs to remove are collected first and then deleted in a best-effort
/// pass: if any individual removal fails the remaining blobs are still
/// attempted so we maximise cleanup. Note that successfully deleted blobs
/// are NOT restored on error — the caller should be aware that an `Err`
/// return may indicate a partially cleaned state where some blobs are
/// already gone.
fn cleanup_image_blobs(digest: &str, repos: &Repositories) -> Result<()> {
    let mut to_remove = vec![digest.to_string()];

    if let OciManifest::Image(img) = &read_manifest(digest)? {
        let referenced_digests = collect_all_referenced_digests(repos)?;

        for layer in &img.layers {
            if !referenced_digests.contains(&layer.digest) {
                to_remove.push(layer.digest.clone());
            }
        }

        if !referenced_digests.contains(&img.config.digest) {
            to_remove.push(img.config.digest.clone());
        }
    }

    let mut errors = Vec::new();
    for blob_digest in &to_remove {
        if let Err(e) = remove_blob(blob_digest) {
            errors.push(format!("{blob_digest}: {e}"));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        bail!(
            "Failed to remove {} blob(s):\n  {}",
            errors.len(),
            errors.join("\n  ")
        )
    }
}

/// Collect all layer and config digests referenced by any remaining manifest.
///
/// Errors are propagated rather than silently ignored: if any manifest cannot
/// be read the reference set would be incomplete and blob cleanup could
/// incorrectly remove shared blobs, turning metadata corruption into
/// irreversible data loss.
fn collect_all_referenced_digests(repos: &Repositories) -> Result<HashSet<String>> {
    let mut all_digests = HashSet::new();

    for (image_ref, manifest_digest) in repos.entries() {
        let manifest = read_manifest(manifest_digest).with_context(|| {
            format!(
                "Cannot determine full reference set: failed to read manifest for '{image_ref}' ({manifest_digest}); aborting blob cleanup to avoid data loss"
            )
        })?;
        if let OciManifest::Image(img) = &manifest {
            for layer in &img.layers {
                all_digests.insert(layer.digest.clone());
            }
            all_digests.insert(img.config.digest.clone());
        }
    }

    Ok(all_digests)
}

fn remove_blob(digest: &str) -> Result<()> {
    let path = ultimate_blob_path(digest)?;
    if !path.exists() {
        return Ok(());
    }
    if path.is_dir() {
        std::fs::remove_dir_all(&path)
            .with_context(|| format!("Failed to remove blob dir {}", path.display()))?;
    } else {
        std::fs::remove_file(&path)
            .with_context(|| format!("Failed to remove blob file {}", path.display()))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// save
// ---------------------------------------------------------------------------

/// Export an image to a tar archive in OCI Image Layout format.
///
/// Because locally stored layers are unpacked directories, we re-compress them
/// and build a fresh manifest so that digests are consistent.
pub fn save_image(args: SaveArgs) -> Result<()> {
    let digest = resolve_image_digest(&args.image_ref)?;
    let manifest = read_manifest(&digest)?;

    let img = match &manifest {
        OciManifest::Image(img) => img,
        _ => bail!("Image indexes are not supported for save"),
    };

    let temp_dir = tempfile::tempdir().context("Failed to create temp directory")?;
    let layout_dir = temp_dir.path();

    let blobs_dir = layout_dir.join("blobs").join("sha256");
    std::fs::create_dir_all(&blobs_dir)?;

    // Write oci-layout
    std::fs::write(
        layout_dir.join("oci-layout"),
        r#"{"imageLayoutVersion":"1.0.0"}"#,
    )?;

    // Parse config blob; we may need to update rootfs.diff_ids if layer tar bytes change.
    let config_src = ultimate_blob_path(&img.config.digest)?;
    let config_content =
        std::fs::read_to_string(&config_src).context("Failed to read image config blob")?;
    let mut config_json: Value =
        serde_json::from_str(&config_content).context("Failed to parse image config blob")?;
    let mut diff_ids = read_config_diff_ids(&config_json)?;
    if diff_ids.len() != img.layers.len() {
        bail!(
            "Image config rootfs.diff_ids length ({}) does not match layer count ({})",
            diff_ids.len(),
            img.layers.len()
        );
    }

    // Re-compress each layer and build new layer descriptors
    let mut new_layers = Vec::new();
    for (index, layer) in img.layers.iter().enumerate() {
        let layer_src = ultimate_blob_path(&layer.digest)?;
        if layer_src.is_dir() {
            let compression_config =
                LayerCompressionConfig::new(layer_src.clone(), blobs_dir.clone());
            let compressor = TarGzCompressor;
            let result = compressor
                .compress_layer(&compression_config)
                .with_context(|| format!("Failed to compress layer {}", layer.digest))?;
            new_layers.push(oci_client::manifest::OciDescriptor {
                media_type: "application/vnd.oci.image.layer.v1.tar+gzip".to_string(),
                digest: format!("sha256:{}", result.gz_sha256sum),
                size: result.gz_size as i64,
                urls: layer.urls.clone(),
                annotations: layer.annotations.clone(),
            });
            diff_ids[index] = format!("sha256:{}", result.tar_sha256sum);
        } else {
            let layer_hash = layer.digest.split_digest()?;
            copy_blob_to_layout(&layer_src, &blobs_dir.join(layer_hash))?;
            new_layers.push(layer.clone());
        }
    }

    write_config_diff_ids(&mut config_json, &diff_ids)?;
    let new_config_json = serde_json::to_vec_pretty(&config_json)?;
    let new_config_hash = sha256_of_bytes(&new_config_json);
    let new_config_digest = format!("sha256:{new_config_hash}");
    std::fs::write(blobs_dir.join(&new_config_hash), &new_config_json)
        .context("Failed to write updated config blob")?;

    let mut new_config = img.config.clone();
    new_config.digest = new_config_digest;
    new_config.size = new_config_json.len() as i64;

    // Build new manifest with updated layer digests
    let new_manifest = OciImageManifest {
        schema_version: img.schema_version,
        media_type: img.media_type.clone(),
        config: new_config,
        layers: new_layers,
        subject: img.subject.clone(),
        artifact_type: img.artifact_type.clone(),
        annotations: img.annotations.clone(),
    };

    let manifest_json = serde_json::to_string_pretty(&new_manifest)?;
    let manifest_hash = sha256_of_bytes(manifest_json.as_bytes());
    let manifest_size = manifest_json.len() as i64;
    std::fs::write(blobs_dir.join(&manifest_hash), &manifest_json)?;

    // Write index.json
    let image_ref_full = normalize_image_ref(&args.image_ref);
    let index = serde_json::json!({
        "schemaVersion": 2,
        "manifests": [{
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "digest": format!("sha256:{manifest_hash}"),
            "size": manifest_size,
            "annotations": {
                "org.opencontainers.image.ref.name": image_ref_full
            }
        }]
    });
    std::fs::write(
        layout_dir.join("index.json"),
        serde_json::to_string_pretty(&index)?,
    )?;

    // Pack the layout directory into a tar archive
    let output_path = Path::new(&args.output);
    if output_path.exists() {
        bail!(
            "Output file '{}' already exists; remove it first or choose a different path",
            args.output
        );
    }
    let tar_result: Result<()> = (|| {
        let output_file = std::fs::File::create(output_path)
            .with_context(|| format!("Failed to create output file {}", args.output))?;
        let mut tar_builder = tar::Builder::new(output_file);
        tar_builder
            .append_dir_all(".", layout_dir)
            .context("Failed to write tar archive")?;
        tar_builder.finish()?;
        Ok(())
    })();

    if let Err(err) = tar_result {
        let _ = std::fs::remove_file(output_path);
        return Err(err);
    }

    println!("Saved image {} to {}", args.image_ref, args.output);
    Ok(())
}

fn copy_blob_to_layout(src: &Path, dst: &Path) -> Result<()> {
    if src.is_dir() {
        copy_dir_recursive(src, dst)?;
    } else {
        std::fs::copy(src, dst)
            .with_context(|| format!("Failed to copy {} to {}", src.display(), dst.display()))?;
    }
    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let target = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn sha256_of_bytes(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

// ---------------------------------------------------------------------------
// load
// ---------------------------------------------------------------------------

/// Import an image from an OCI Image Layout tar archive into local storage.
pub fn load_image(args: LoadArgs) -> Result<()> {
    let input_path = Path::new(&args.input);
    if !input_path.exists() {
        bail!("Input file '{}' does not exist", args.input);
    }

    let temp_dir = tempfile::tempdir().context("Failed to create temp directory")?;
    let layout_dir = temp_dir.path();

    // Extract tar with path validation to avoid traversal attacks.
    extract_oci_layout_archive(input_path, layout_dir)?;

    // Validate oci-layout
    let oci_layout_path = layout_dir.join("oci-layout");
    if !oci_layout_path.exists() {
        bail!("Invalid OCI Image Layout: missing oci-layout file");
    }

    // Read index.json to find manifest digest and optional image name
    let index_path = layout_dir.join("index.json");
    let index_content =
        std::fs::read_to_string(&index_path).context("Failed to read index.json")?;
    let index: serde_json::Value =
        serde_json::from_str(&index_content).context("Failed to parse index.json")?;

    let manifests = index["manifests"]
        .as_array()
        .ok_or_else(|| anyhow!("Invalid index.json: missing manifests array"))?;

    if manifests.is_empty() {
        bail!("No manifests found in index.json");
    }

    let blobs_dir = layout_dir.join("blobs").join("sha256");
    if let Some(tag) = args.tag {
        if manifests.len() > 1 {
            eprintln!(
                "Warning: archive contains {} manifests, but --tag was provided; only the first manifest will be loaded.",
                manifests.len()
            );
        }
        import_manifest_entry(&blobs_dir, &manifests[0], &tag)?;
    } else {
        let mut loaded_count = 0usize;
        let mut skipped_without_name = 0usize;

        for manifest_entry in manifests {
            let image_name = manifest_entry["annotations"]["org.opencontainers.image.ref.name"]
                .as_str()
                .map(str::to_owned);

            match image_name {
                Some(name) => {
                    import_manifest_entry(&blobs_dir, manifest_entry, &name)?;
                    loaded_count += 1;
                }
                None => {
                    skipped_without_name += 1;
                }
            }
        }

        if skipped_without_name > 0 {
            eprintln!(
                "Warning: skipped {skipped_without_name} manifest(s) without org.opencontainers.image.ref.name annotation. Use --tag to import one explicitly."
            );
        }

        if loaded_count == 0 {
            bail!("No manifest was imported. Use --tag to specify an image reference.");
        }
    }

    Ok(())
}

fn import_blob(blobs_dir: &Path, digest: &str) -> Result<()> {
    let hash = digest.split_digest()?;
    let src = blobs_dir.join(hash);
    let dst = ultimate_blob_path(digest)?;
    if dst.exists() {
        return Ok(());
    }
    std::fs::copy(&src, &dst).with_context(|| format!("Failed to import blob {digest}"))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn read_image_config_value(config_digest: &str) -> Result<Option<Value>> {
    let config_path = ultimate_blob_path(config_digest)?;
    if !config_path.exists() {
        return Ok(None);
    }
    let config_content = std::fs::read_to_string(&config_path)
        .with_context(|| format!("Failed to read image config from {}", config_path.display()))?;
    let config_json: Value = serde_json::from_str(&config_content).with_context(|| {
        format!(
            "Failed to parse image config from {}",
            config_path.display()
        )
    })?;
    Ok(Some(config_json))
}

fn read_config_diff_ids(config_json: &Value) -> Result<Vec<String>> {
    let diff_ids = config_json
        .get("rootfs")
        .and_then(|v| v.get("diff_ids"))
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("Image config missing rootfs.diff_ids"))?;

    let mut result = Vec::with_capacity(diff_ids.len());
    for value in diff_ids {
        let diff_id = value
            .as_str()
            .ok_or_else(|| anyhow!("Image config has non-string rootfs.diff_ids entry"))?;
        result.push(diff_id.to_string());
    }
    Ok(result)
}

fn write_config_diff_ids(config_json: &mut Value, diff_ids: &[String]) -> Result<()> {
    let rootfs = config_json
        .get_mut("rootfs")
        .and_then(|v| v.as_object_mut())
        .ok_or_else(|| anyhow!("Image config missing rootfs object"))?;
    rootfs.insert(
        "diff_ids".to_string(),
        Value::Array(diff_ids.iter().cloned().map(Value::String).collect()),
    );
    Ok(())
}

fn extract_oci_layout_archive(input_path: &Path, layout_dir: &Path) -> Result<()> {
    let tar_file = std::fs::File::open(input_path).context("Failed to open input tar file")?;
    let mut archive = tar::Archive::new(tar_file);
    archive.set_preserve_permissions(false);
    archive.set_unpack_xattrs(false);

    let entries = archive
        .entries()
        .context("Failed to read tar entries from archive")?;
    for entry in entries {
        let mut entry = entry.context("Failed to read tar entry")?;
        let entry_path = entry.path().context("Invalid tar entry path")?.into_owned();
        validate_archive_entry_path(&entry_path)?;
        let unpacked = entry
            .unpack_in(layout_dir)
            .with_context(|| format!("Failed to unpack entry {}", entry_path.display()))?;
        if !unpacked {
            bail!(
                "Unsafe tar entry path '{}' escapes destination directory",
                entry_path.display()
            );
        }
    }

    Ok(())
}

fn validate_archive_entry_path(path: &Path) -> Result<()> {
    if path.is_absolute() {
        bail!("Archive contains absolute path '{}'", path.display());
    }
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => {
                bail!("Archive contains unsafe path '{}'", path.display())
            }
            Component::CurDir | Component::Normal(_) => {}
        }
    }
    Ok(())
}

fn import_manifest_entry(blobs_dir: &Path, manifest_entry: &Value, image_ref: &str) -> Result<()> {
    let manifest_digest = manifest_entry["digest"]
        .as_str()
        .ok_or_else(|| anyhow!("Invalid manifest entry: missing digest"))?;
    if !manifest_digest.starts_with("sha256:") {
        bail!(
            "Unsupported manifest digest algorithm: {manifest_digest} (only sha256 is supported)"
        );
    }
    let manifest_hash = manifest_digest.split_digest()?;
    let manifest_blob_path = blobs_dir.join(manifest_hash);
    let manifest_content =
        std::fs::read_to_string(&manifest_blob_path).context("Failed to read manifest blob")?;
    let actual_hash = sha256_of_bytes(manifest_content.as_bytes());
    if actual_hash != manifest_hash {
        bail!("Manifest digest mismatch: expected {manifest_hash}, got {actual_hash}");
    }
    let manifest: OciImageManifest =
        serde_json::from_str(&manifest_content).context("Failed to parse manifest")?;

    import_blob(blobs_dir, &manifest.config.digest)?;
    for layer in &manifest.layers {
        let layer_hash = layer.digest.split_digest()?;
        let src = blobs_dir.join(layer_hash);
        let dst = ultimate_blob_path(&layer.digest)?;

        if dst.exists() {
            continue;
        }

        let media_type = get_media_type(&layer.media_type);
        match media_type {
            MediaType::Tar | MediaType::TarGzip => {
                media_type.unpack(&src, &dst)?;
            }
            MediaType::Other => {
                std::fs::copy(&src, &dst)?;
            }
        }
    }

    let manifest_dst = ultimate_blob_path(manifest_digest)?;
    if !manifest_dst.exists() {
        std::fs::write(&manifest_dst, &manifest_content)?;
    }

    let full_ref = normalize_image_ref(image_ref);
    Repositories::add_store(vec![(full_ref.clone(), manifest_digest.to_string())])?;
    println!("Loaded image: {full_ref}");
    Ok(())
}

/// Resolve an image reference to its manifest digest, trying both
/// the raw input and the normalized form.
fn resolve_image_digest(image_ref: &str) -> Result<String> {
    let repos = Repositories::load()?;

    if let Some(digest) = repos.get(image_ref)? {
        return Ok(digest.to_string());
    }

    let normalized = normalize_image_ref(image_ref);
    if let Some(digest) = repos.get(&normalized)? {
        return Ok(digest.to_string());
    }

    bail!(
        "Image '{}' not found locally (also tried '{}')",
        image_ref,
        normalized
    )
}

/// Normalize an image reference: ensure it has a namespace and tag.
fn normalize_image_ref(image_ref: &str) -> String {
    let (name, digest) = match image_ref.split_once('@') {
        Some((n, d)) => (n, Some(d)),
        None => (image_ref, None),
    };

    let last_slash = name.rfind('/');
    let last_colon = name.rfind(':');
    let has_tag = match (last_colon, last_slash) {
        (Some(c), Some(s)) => c > s,
        (Some(_), None) => true,
        _ => false,
    };

    let normalized = if has_tag {
        let (repo, tag) = name.rsplit_once(':').unwrap();
        if last_slash.is_none() && tag.chars().all(|c| c.is_ascii_digit()) {
            full_image_ref(name, Some("latest"))
        } else {
            full_image_ref(repo, Some(tag))
        }
    } else {
        full_image_ref(name, Some("latest"))
    };

    match digest {
        Some(d) => format!("{normalized}@{d}"),
        None => normalized,
    }
}

fn resolve_remove_target(input: &str, repos: &Repositories) -> Result<String> {
    if repos.get(input)?.is_some() {
        return Ok(input.to_string());
    }

    let normalized = normalize_image_ref(input);
    if repos.get(&normalized)?.is_some() {
        return Ok(normalized);
    }

    let mut matches = repos
        .entries()
        .into_iter()
        .filter(|(_, digest)| digest_matches_identifier(digest, input))
        .map(|(image_ref, _)| image_ref.clone())
        .collect::<Vec<_>>();

    matches.sort();
    matches.dedup();

    match matches.len() {
        1 => Ok(matches.remove(0)),
        0 => bail!("Image '{}' not found locally", input),
        _ => bail!(
            "Image identifier '{}' is ambiguous, matching refs: {}",
            input,
            matches.join(", ")
        ),
    }
}

fn digest_matches_identifier(digest: &str, identifier: &str) -> bool {
    if identifier.starts_with("sha256:") {
        return digest == identifier;
    }
    let Ok(hash) = digest.split_digest() else {
        return false;
    };
    if !identifier.chars().all(|c| c.is_ascii_hexdigit()) {
        return false;
    }
    hash.eq_ignore_ascii_case(identifier) || hash.starts_with(identifier)
}

fn split_image_ref(image_ref: &str) -> (&str, &str) {
    match image_ref.rsplit_once(':') {
        Some((repo, tag)) => (repo, tag),
        None => (image_ref, "<none>"),
    }
}

fn short_id(digest: &str) -> String {
    match digest.split_digest() {
        Ok(hash) => hash.chars().take(12).collect(),
        Err(_) => digest.to_string(),
    }
}

/// Return (size, created) for display, reading the manifest only once.
fn get_image_metadata(digest: &str) -> Result<(String, String)> {
    let manifest = read_manifest(digest)?;

    let total_bytes: i64 = match &manifest {
        OciManifest::Image(img) => img.layers.iter().map(|l| l.size).sum::<i64>() + img.config.size,
        OciManifest::ImageIndex(idx) => idx.manifests.iter().map(|m| m.size).sum(),
    };
    if total_bytes < 0 {
        bail!("invalid negative size in manifest for digest {}", digest);
    }
    let size = format_size(total_bytes as u64);

    let created = if let OciManifest::Image(img) = &manifest
        && let Some(config) = read_image_config_value(&img.config.digest)?
        && let Some(ts) = config.get("created").and_then(|v| v.as_str())
    {
        ts.to_string()
    } else {
        let path = ultimate_blob_path(digest)?;
        let metadata = std::fs::metadata(&path)
            .with_context(|| format!("Failed to read metadata for {}", path.display()))?;
        let modified = metadata
            .modified()
            .with_context(|| "Failed to get modified time")?;
        let datetime: DateTime<Local> = modified.into();
        datetime.format("%Y-%m-%d %H:%M:%S").to_string()
    };

    Ok((size, created))
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_image_ref_defaults() {
        assert_eq!(normalize_image_ref("ubuntu"), "library/ubuntu:latest");
        assert_eq!(normalize_image_ref("library/nginx"), "library/nginx:latest");
    }

    #[test]
    fn test_normalize_image_ref_keep_tag() {
        assert_eq!(normalize_image_ref("ubuntu:22.04"), "library/ubuntu:22.04");
        assert_eq!(
            normalize_image_ref("library/nginx:1.25"),
            "library/nginx:1.25"
        );
    }

    #[test]
    fn test_normalize_image_ref_host_port() {
        // Bare host:port without image path (e.g. "localhost:5000") is not a
        // valid image reference — neither Docker nor podman accept it.
        // We only guarantee correct handling for the common patterns:
        assert_eq!(
            normalize_image_ref("localhost:5000/myimage"),
            "localhost:5000/myimage:latest"
        );
        assert_eq!(
            normalize_image_ref("localhost:5000/myimage:v1"),
            "localhost:5000/myimage:v1"
        );
    }

    #[test]
    fn test_split_image_ref() {
        assert_eq!(
            split_image_ref("library/ubuntu:latest"),
            ("library/ubuntu", "latest")
        );
        assert_eq!(
            split_image_ref("library/ubuntu"),
            ("library/ubuntu", "<none>")
        );
    }

    #[test]
    fn test_short_id() {
        assert_eq!(
            short_id("sha256:1234567890abcdef1234567890abcdef"),
            "1234567890ab"
        );
        assert_eq!(short_id("invalid-digest"), "invalid-digest");
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(999), "999 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1024 * 1024), "1.0 MB");
    }

    #[test]
    fn test_digest_matches_identifier() {
        let digest = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        assert!(digest_matches_identifier(
            digest,
            "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        ));
        assert!(digest_matches_identifier(digest, "0123456789ab"));
        assert!(digest_matches_identifier(
            digest,
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        ));
        assert!(!digest_matches_identifier(digest, "not-a-digest"));
    }

    #[test]
    fn test_validate_archive_entry_path() {
        assert!(validate_archive_entry_path(Path::new("blobs/sha256/abcd")).is_ok());
        assert!(validate_archive_entry_path(Path::new("../etc/passwd")).is_err());
        assert!(validate_archive_entry_path(Path::new("/tmp/evil")).is_err());
    }
}
