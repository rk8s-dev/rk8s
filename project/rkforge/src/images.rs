use crate::compressor::tar_gz_compressor::TarGzCompressor;
use crate::compressor::{LayerCompressionConfig, LayerCompressor};
use crate::config::meta::Repositories;
use crate::pull::media::{MediaType, get_media_type};
use crate::storage::{DigestExt, full_image_ref, read_manifest, ultimate_blob_path};
use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Local};
use clap::Parser;
use oci_client::manifest::{OciImageManifest, OciManifest};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
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
        let size = get_image_size(digest).unwrap_or_else(|_| "N/A".to_string());
        let created = get_created_time(digest).unwrap_or_else(|_| "N/A".to_string());

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
            println!("{}", serde_json::to_string_pretty(img)?);

            let config_path = ultimate_blob_path(&img.config.digest)?;
            if config_path.exists() {
                let config_content = std::fs::read_to_string(&config_path)
                    .with_context(|| "Failed to read image config")?;
                if let Ok(config_json) = serde_json::from_str::<serde_json::Value>(&config_content)
                {
                    println!("\n--- Image Config ---");
                    println!("{}", serde_json::to_string_pretty(&config_json)?);
                }
            }
        }
        OciManifest::ImageIndex(idx) => {
            println!("{}", serde_json::to_string_pretty(idx)?);
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

/// Remove an image from local storage
pub fn remove_image(args: RmiArgs) -> Result<()> {
    let image_ref = normalize_image_ref(&args.image_ref);

    let mut repos = Repositories::load()?;
    let digest = repos
        .remove(&image_ref)
        .ok_or_else(|| anyhow!("Image '{}' not found locally", args.image_ref))?;

    let still_referenced = repos.digests().iter().any(|d| **d == digest);

    if !still_referenced {
        if let Err(e) = cleanup_image_blobs(&digest, &repos) {
            if args.force {
                eprintln!("Warning: blob cleanup failed: {e}");
            } else {
                repos.add(&image_ref, &digest);
                repos.store()?;
                return Err(e.context("Failed to clean up blobs; tag restored"));
            }
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
fn cleanup_image_blobs(digest: &str, repos: &Repositories) -> Result<()> {
    let manifest = read_manifest(digest)?;

    if let OciManifest::Image(img) = &manifest {
        let referenced_digests = collect_all_referenced_digests(repos)?;

        for layer in &img.layers {
            if !referenced_digests.contains(&layer.digest) {
                remove_blob(&layer.digest)?;
            }
        }

        if !referenced_digests.contains(&img.config.digest) {
            remove_blob(&img.config.digest)?;
        }
    }

    remove_blob(digest)?;
    Ok(())
}

/// Collect all layer and config digests referenced by any remaining manifest.
fn collect_all_referenced_digests(repos: &Repositories) -> Result<Vec<String>> {
    let mut all_digests = Vec::new();

    for (_ref, manifest_digest) in repos.entries() {
        if let Ok(OciManifest::Image(img)) = read_manifest(manifest_digest) {
            for layer in &img.layers {
                all_digests.push(layer.digest.clone());
            }
            all_digests.push(img.config.digest.clone());
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

    // Copy config blob
    let config_src = ultimate_blob_path(&img.config.digest)?;
    let config_hash = img.config.digest.split_digest()?;
    copy_blob_to_layout(&config_src, &blobs_dir.join(config_hash))?;

    // Re-compress each layer and build new layer descriptors
    let mut new_layers = Vec::new();
    for layer in &img.layers {
        let layer_src = ultimate_blob_path(&layer.digest)?;
        if layer_src.is_dir() {
            let compression_config =
                LayerCompressionConfig::new(layer_src.clone(), blobs_dir.clone());
            let compressor = TarGzCompressor;
            let result = compressor
                .compress_layer(&compression_config)
                .with_context(|| {
                    format!("Failed to compress layer {}", layer.digest)
                })?;
            new_layers.push(oci_client::manifest::OciDescriptor {
                media_type: "application/vnd.oci.image.layer.v1.tar+gzip".to_string(),
                digest: format!("sha256:{}", result.gz_sha256sum),
                size: result.gz_size as i64,
                urls: layer.urls.clone(),
                annotations: layer.annotations.clone(),
            });
        } else {
            let layer_hash = layer.digest.split_digest()?;
            copy_blob_to_layout(&layer_src, &blobs_dir.join(layer_hash))?;
            new_layers.push(layer.clone());
        }
    }

    // Build new manifest with updated layer digests
    let new_manifest = OciImageManifest {
        schema_version: img.schema_version,
        media_type: img.media_type.clone(),
        config: img.config.clone(),
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
    let output_file = std::fs::File::create(&args.output)
        .with_context(|| format!("Failed to create output file {}", args.output))?;
    let mut tar_builder = tar::Builder::new(output_file);
    tar_builder
        .append_dir_all(".", layout_dir)
        .context("Failed to write tar archive")?;
    tar_builder.finish()?;

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

    // Extract tar
    let tar_file =
        std::fs::File::open(input_path).with_context(|| "Failed to open input tar file")?;
    let mut archive = tar::Archive::new(tar_file);
    archive
        .unpack(layout_dir)
        .context("Failed to extract tar archive")?;

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

    let manifest_entry = &manifests[0];
    let manifest_digest = manifest_entry["digest"]
        .as_str()
        .ok_or_else(|| anyhow!("Invalid manifest entry: missing digest"))?;

    let image_name_from_annotation = manifest_entry["annotations"]
        ["org.opencontainers.image.ref.name"]
        .as_str()
        .map(String::from);

    // Determine the image reference to use
    let image_ref = args
        .tag
        .or(image_name_from_annotation)
        .ok_or_else(|| anyhow!("No image reference found; use --tag to specify one"))?;

    // Read manifest from blobs/
    let manifest_hash = manifest_digest.split_digest()?;
    let blobs_dir = layout_dir.join("blobs").join("sha256");
    let manifest_blob_path = blobs_dir.join(manifest_hash);
    let manifest_content =
        std::fs::read_to_string(&manifest_blob_path).context("Failed to read manifest blob")?;
    let manifest: OciImageManifest =
        serde_json::from_str(&manifest_content).context("Failed to parse manifest")?;

    // Import config blob
    import_blob(&blobs_dir, &manifest.config.digest)?;

    // Import layer blobs
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

    // Store manifest blob to local storage
    let manifest_dst = ultimate_blob_path(manifest_digest)?;
    if !manifest_dst.exists() {
        std::fs::write(&manifest_dst, &manifest_content)?;
    }

    // Update repositories metadata
    Repositories::add_store(vec![(image_ref.clone(), manifest_digest.to_string())])?;

    println!("Loaded image: {image_ref}");
    Ok(())
}

fn import_blob(blobs_dir: &Path, digest: &str) -> Result<()> {
    let hash = digest.split_digest()?;
    let src = blobs_dir.join(hash);
    let dst = ultimate_blob_path(digest)?;
    if dst.exists() {
        return Ok(());
    }
    std::fs::copy(&src, &dst)
        .with_context(|| format!("Failed to import blob {digest}"))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

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
    if image_ref.contains(':') {
        let (repo, tag) = image_ref.rsplit_once(':').unwrap();
        full_image_ref(repo, Some(tag))
    } else {
        full_image_ref(image_ref, Some("latest"))
    }
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

fn get_image_size(digest: &str) -> Result<String> {
    let manifest = read_manifest(digest)?;

    let total_bytes: i64 = match &manifest {
        OciManifest::Image(img) => {
            img.layers.iter().map(|l| l.size).sum::<i64>() + img.config.size
        }
        OciManifest::ImageIndex(idx) => idx.manifests.iter().map(|m| m.size).sum(),
    };

    Ok(format_size(total_bytes as u64))
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

fn get_created_time(digest: &str) -> Result<String> {
    let path = ultimate_blob_path(digest)?;
    let metadata = std::fs::metadata(&path)
        .with_context(|| format!("Failed to read metadata for {}", path.display()))?;
    let modified = metadata
        .modified()
        .with_context(|| "Failed to get modified time")?;
    let datetime: DateTime<Local> = modified.into();
    Ok(datetime.format("%Y-%m-%d %H:%M:%S").to_string())
}
