pub mod build_runtime;
pub mod config;
pub mod context;
pub mod execute;
pub mod executor;
mod metadata;
pub mod stage_executor;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use crate::compressor::tar_gz_compressor::TarGzCompressor;
use crate::image::build_runtime::{
    BuildHostEntry, BuildNetworkMode, BuildSecret, BuildSshAgent, BuildUlimit, BuildUlimitResource,
    BuildUlimitValue, normalize_cgroup_parent,
};
use crate::image::executor::Executor;
use crate::image::metadata::{BuildMetadata, write_metadata_file};
use crate::push::push_from_layout;
use anyhow::{Context, Result, bail};
use clap::{Parser, ValueEnum};
use dockerfile_parser::Dockerfile;
use oci_client::manifest::OciImageIndex;
use oci_spec::distribution::Reference;
use rand::{Rng, distr::Alphanumeric};

pub static BLOBS: &str = "blobs/sha256";

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum BuildProgressMode {
    Auto,
    Tty,
    Plain,
}

#[derive(Parser, Debug)]
pub struct BuildArgs {
    /// Dockerfile or Containerfile
    #[arg(short, long, value_name = "FILE")]
    pub file: Option<PathBuf>,

    /// Image identifier (format: "[registry/]repository[:tag]"), can be set multiple times
    #[arg(short = 't', long = "tag", value_name = "NAME")]
    pub tags: Vec<String>,

    /// Turn verbose logging on
    #[arg(short, long)]
    pub verbose: bool,

    /// Use libfuse-rs or linux mount
    #[arg(short, long)]
    pub libfuse: bool,

    /// Output directory for the image
    #[arg(short, long, value_name = "DIR")]
    pub output_dir: Option<String>,

    /// Build to a specific stage in the Dockerfile
    #[arg(long, value_name = "TARGET")]
    pub target: Option<String>,

    /// Set build-time variables (format: KEY=VALUE), can be set multiple times
    #[arg(long = "build-arg", value_name = "KEY=VALUE")]
    pub build_args: Vec<String>,

    /// Do not use cache when building the image
    #[arg(long)]
    pub no_cache: bool,

    /// Suppress build output
    #[arg(short = 'q', long)]
    pub quiet: bool,

    /// Write the resulting image digest to the file
    #[arg(long, value_name = "FILE")]
    pub iidfile: Option<PathBuf>,

    /// Set metadata for an image (format: KEY=VALUE), can be set multiple times
    #[arg(long = "label", value_name = "KEY=VALUE")]
    pub labels: Vec<String>,

    /// Set type of progress output (auto, tty, plain)
    #[arg(long, value_enum, default_value = "auto")]
    pub progress: BuildProgressMode,

    /// Add a custom host-to-IP mapping (format: HOST:IP), can be set multiple times
    #[arg(long = "add-host", value_name = "HOST:IP", value_parser = parse_add_host_option)]
    pub add_hosts: Vec<BuildHostEntry>,

    /// Shared memory size for build containers (e.g. 64m, 1g)
    #[arg(long = "shm-size", value_name = "SIZE", value_parser = parse_shm_size)]
    pub shm_size: Option<u64>,

    /// Ulimit options (format: NAME=SOFT:HARD), can be set multiple times
    #[arg(long = "ulimit", value_name = "NAME=SOFT:HARD", value_parser = parse_ulimit_option)]
    pub ulimits: Vec<BuildUlimit>,

    /// Build result metadata output file (JSON)
    #[arg(long = "metadata-file", value_name = "FILE")]
    pub metadata_file: Option<PathBuf>,

    /// Push image to registry after a successful build
    #[arg(long)]
    pub push: bool,

    /// Set metadata annotation for OCI manifest descriptors (format: KEY=VALUE), can be set multiple times
    #[arg(long = "annotation", value_name = "KEY=VALUE")]
    pub annotations: Vec<String>,

    /// Set networking mode for RUN instructions (default, none, host). In current single-node runtime, default behaves the same as host.
    #[arg(long, value_enum, default_value = "default")]
    pub network: BuildNetworkMode,

    /// Set the parent cgroup for RUN instructions (requires cgroup v2 + sufficient privileges)
    #[arg(long = "cgroup-parent", value_name = "PARENT")]
    pub cgroup_parent: Option<String>,

    /// Disable cache for specific stages by stage name or index (e.g. --no-cache-filter builder --no-cache-filter 0), can be set multiple times
    #[arg(long = "no-cache-filter", value_name = "STAGE")]
    pub no_cache_filter: Vec<String>,

    /// Secret to expose to the build (format: "id=mysecret[,src=/local/secret]"), can be set multiple times
    #[arg(long = "secret", value_name = "id=...[,src=...]", value_parser = parse_secret_option)]
    pub secrets: Vec<BuildSecret>,

    /// SSH agent socket or keys to expose to the build (format: "default|<id>[=<socket>]"), can be set multiple times
    #[arg(long = "ssh", value_name = "default|<id>[=<socket>]", value_parser = parse_ssh_option)]
    pub ssh: Vec<BuildSshAgent>,

    /// Build context. Defaults to the directory of the Dockerfile.
    #[arg(default_value = ".")]
    pub context: PathBuf,
}

fn parse_add_host_option(raw: &str) -> std::result::Result<BuildHostEntry, String> {
    let (host, ip) = raw
        .split_once(':')
        .ok_or_else(|| format!("invalid --add-host value `{raw}`: expected format HOST:IP"))?;

    let host = host.trim();
    if host.is_empty() {
        return Err(format!(
            "invalid --add-host value `{raw}`: host must not be empty"
        ));
    }
    if host.chars().any(char::is_whitespace) {
        return Err(format!(
            "invalid --add-host value `{raw}`: host must not contain spaces"
        ));
    }

    let ip = ip.trim();
    if ip.is_empty() {
        return Err(format!(
            "invalid --add-host value `{raw}`: IP must not be empty"
        ));
    }
    let ip = ip
        .parse::<IpAddr>()
        .map_err(|e| format!("invalid --add-host value `{raw}`: invalid IP `{ip}`: {e}"))?;

    Ok(BuildHostEntry {
        host: host.to_string(),
        ip,
    })
}

fn parse_shm_size(raw: &str) -> std::result::Result<u64, String> {
    let value = raw.trim().to_ascii_lowercase();
    if value.is_empty() {
        return Err("invalid --shm-size value: empty input".to_string());
    }

    let unit_start = value
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(value.len());
    let (num_part, unit_part) = value.split_at(unit_start);

    if num_part.is_empty() {
        return Err(format!(
            "invalid --shm-size value `{raw}`: expected positive number with optional unit"
        ));
    }

    let number = num_part.parse::<u64>().map_err(|e| {
        format!("invalid --shm-size value `{raw}`: invalid number `{num_part}`: {e}")
    })?;
    let multiplier = match unit_part {
        "" | "b" => 1_u64,
        "k" | "kb" | "ki" | "kib" => 1024_u64,
        "m" | "mb" | "mi" | "mib" => 1024_u64.pow(2),
        "g" | "gb" | "gi" | "gib" => 1024_u64.pow(3),
        "t" | "tb" | "ti" | "tib" => 1024_u64.pow(4),
        "p" | "pb" | "pi" | "pib" => 1024_u64.pow(5),
        _ => {
            return Err(format!(
                "invalid --shm-size value `{raw}`: unsupported unit `{unit_part}`"
            ));
        }
    };

    let size_bytes = number
        .checked_mul(multiplier)
        .ok_or_else(|| format!("invalid --shm-size value `{raw}`: value exceeds u64 range"))?;
    if size_bytes == 0 {
        return Err(format!(
            "invalid --shm-size value `{raw}`: size must be greater than 0"
        ));
    }
    Ok(size_bytes)
}

fn parse_ulimit_value(raw: &str, full_raw: &str) -> std::result::Result<BuildUlimitValue, String> {
    let raw = raw.trim();
    if raw.eq_ignore_ascii_case("unlimited") || raw.eq_ignore_ascii_case("infinity") {
        return Ok(BuildUlimitValue::Unlimited);
    }

    let value = raw
        .parse::<u64>()
        .map_err(|e| format!("invalid --ulimit value `{full_raw}`: invalid limit `{raw}`: {e}"))?;
    Ok(BuildUlimitValue::Value(value))
}

fn parse_ulimit_option(raw: &str) -> std::result::Result<BuildUlimit, String> {
    let (name, limits) = raw
        .split_once('=')
        .ok_or_else(|| format!("invalid --ulimit value `{raw}`: expected format NAME=SOFT:HARD"))?;

    let name = name.trim().to_ascii_lowercase();
    if name.is_empty() {
        return Err(format!(
            "invalid --ulimit value `{raw}`: name must not be empty"
        ));
    }

    let resource = BuildUlimitResource::from_name(&name)
        .ok_or_else(|| format!("invalid --ulimit value `{raw}`: unsupported resource `{name}`"))?;

    let (soft_raw, hard_raw) = limits
        .split_once(':')
        .ok_or_else(|| format!("invalid --ulimit value `{raw}`: expected format NAME=SOFT:HARD"))?;
    let soft = parse_ulimit_value(soft_raw, raw)?;
    let hard = parse_ulimit_value(hard_raw, raw)?;

    match (soft, hard) {
        (BuildUlimitValue::Unlimited, BuildUlimitValue::Value(_)) => {
            return Err(format!(
                "invalid --ulimit value `{raw}`: soft limit cannot be unlimited when hard limit is finite"
            ));
        }
        (BuildUlimitValue::Value(soft), BuildUlimitValue::Value(hard)) if soft > hard => {
            return Err(format!(
                "invalid --ulimit value `{raw}`: soft limit must be <= hard limit"
            ));
        }
        _ => {}
    }

    Ok(BuildUlimit {
        resource,
        soft,
        hard,
    })
}

fn parse_secret_option(raw: &str) -> std::result::Result<BuildSecret, String> {
    let mut id = None;
    let mut src = None;

    for part in raw.split(',') {
        let (key, value) = part.split_once('=').ok_or_else(|| {
            format!("invalid --secret value `{raw}`: expected comma-separated KEY=VALUE pairs")
        })?;
        match key.trim() {
            "id" => {
                let v = value.trim();
                if v.is_empty() {
                    return Err(format!(
                        "invalid --secret value `{raw}`: id must not be empty"
                    ));
                }
                id = Some(v.to_string());
            }
            "src" => {
                let v = value.trim();
                if v.is_empty() {
                    return Err(format!(
                        "invalid --secret value `{raw}`: src must not be empty"
                    ));
                }
                src = Some(PathBuf::from(v));
            }
            other => {
                return Err(format!(
                    "invalid --secret value `{raw}`: unknown key `{other}`"
                ));
            }
        }
    }

    let id = id.ok_or_else(|| format!("invalid --secret value `{raw}`: missing required `id`"))?;

    if id.contains('/') || id.contains('\\') || id.contains("..") || id.contains('\0') {
        return Err(format!(
            "invalid --secret value `{raw}`: id `{id}` must not contain path separators or `..`"
        ));
    }

    let src = src.unwrap_or_else(|| PathBuf::from(&id));

    if !src.exists() {
        return Err(format!(
            "invalid --secret value `{raw}`: source file `{}` does not exist",
            src.display()
        ));
    }
    if !src.is_file() {
        return Err(format!(
            "invalid --secret value `{raw}`: source `{}` is not a regular file",
            src.display()
        ));
    }

    let src = src
        .canonicalize()
        .map_err(|e| format!("invalid --secret value `{raw}`: failed to resolve path: {e}"))?;

    Ok(BuildSecret { id, src })
}

fn parse_ssh_option(raw: &str) -> std::result::Result<BuildSshAgent, String> {
    let (id, socket_path) = if let Some((id, path)) = raw.split_once('=') {
        let id = id.trim();
        if id.is_empty() {
            return Err(format!("invalid --ssh value `{raw}`: id must not be empty"));
        }
        let path = path.trim();
        if path.is_empty() {
            return Err(format!(
                "invalid --ssh value `{raw}`: socket path must not be empty"
            ));
        }
        (id.to_string(), PathBuf::from(path))
    } else {
        let id = raw.trim();
        if id.is_empty() {
            return Err("invalid --ssh value: id must not be empty".to_string());
        }
        let socket_path = std::env::var("SSH_AUTH_SOCK")
            .map(PathBuf::from)
            .map_err(|_| {
                format!(
                    "invalid --ssh value `{raw}`: no socket path specified and SSH_AUTH_SOCK is not set"
                )
            })?;
        (id.to_string(), socket_path)
    };

    if !socket_path.exists() {
        return Err(format!(
            "invalid --ssh value `{raw}`: socket `{}` does not exist",
            socket_path.display()
        ));
    }

    use std::os::unix::fs::FileTypeExt;
    let meta = std::fs::symlink_metadata(&socket_path).map_err(|e| {
        format!(
            "invalid --ssh value `{raw}`: failed to inspect `{}`: {e}",
            socket_path.display()
        )
    })?;
    if !meta.file_type().is_socket() {
        return Err(format!(
            "invalid --ssh value `{raw}`: `{}` is not a unix socket",
            socket_path.display()
        ));
    }

    Ok(BuildSshAgent { id, socket_path })
}

fn parse_dockerfile<P: AsRef<Path>>(dockerfile_path: P) -> Result<Dockerfile> {
    let dockerfile_path = dockerfile_path.as_ref().to_path_buf();
    let dockerfile_content = fs::read_to_string(&dockerfile_path)
        .with_context(|| format!("Failed to read Dockerfile: {}", dockerfile_path.display()))?;
    let dockerfile = Dockerfile::parse(&dockerfile_content)
        .with_context(|| format!("Failed to parse Dockerfile: {}", dockerfile_path.display()))?;
    Ok(dockerfile)
}

fn resolve_dockerfile_path(build_args: &BuildArgs) -> Result<PathBuf> {
    if let Some(path) = &build_args.file {
        return Ok(path.clone());
    }

    let dockerfile = build_args.context.join("Dockerfile");
    if dockerfile.exists() {
        return Ok(dockerfile);
    }

    let containerfile = build_args.context.join("Containerfile");
    if containerfile.exists() {
        return Ok(containerfile);
    }

    bail!(
        "failed to locate Dockerfile in context `{}`: expected `Dockerfile` or `Containerfile`",
        build_args.context.display()
    );
}

fn parse_key_value_options(
    options: &[String],
    option_name: &str,
) -> Result<HashMap<String, String>> {
    let mut parsed = HashMap::new();
    for raw in options {
        let (key, value) = raw.split_once('=').with_context(|| {
            format!("invalid {option_name} value `{raw}`: expected format KEY=VALUE")
        })?;

        let key = key.trim();
        if key.is_empty() {
            bail!("invalid {option_name} value `{raw}`: key must not be empty");
        }

        parsed.insert(key.to_string(), value.to_string());
    }
    Ok(parsed)
}

fn parse_global_args(dockerfile: &Dockerfile) -> HashMap<String, Option<String>> {
    dockerfile
        .global_args
        .iter()
        .map(|arg| {
            let key = arg.name.content.clone();
            let value = arg.value.as_ref().map(|v| v.content.clone());
            (key, value)
        })
        .collect()
}

fn read_primary_image_digest<P: AsRef<Path>>(
    image_output_dir: P,
    preferred_ref_name: Option<&str>,
) -> Result<String> {
    let index_path = image_output_dir.as_ref().join("index.json");
    let index_content = fs::read_to_string(&index_path)
        .with_context(|| format!("Failed to read {}", index_path.display()))?;
    let image_index = serde_json::from_str::<OciImageIndex>(&index_content)
        .with_context(|| format!("Failed to parse {}", index_path.display()))?;

    if let Some(preferred_ref_name) = preferred_ref_name
        && let Some(descriptor) = image_index.manifests.iter().find(|descriptor| {
            descriptor
                .annotations
                .as_ref()
                .and_then(|annotations| annotations.get("org.opencontainers.image.ref.name"))
                .is_some_and(|value| value == preferred_ref_name)
        })
    {
        return Ok(descriptor.digest.clone());
    }

    let digest = image_index
        .manifests
        .first()
        .map(|descriptor| descriptor.digest.clone())
        .context("index.json contains no manifest descriptors")?;
    Ok(digest)
}

fn write_iidfile<P: AsRef<Path>>(path: P, digest: &str) -> Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create parent directory {}", parent.display()))?;
    }
    let mut file =
        fs::File::create(path).with_context(|| format!("Failed to create {}", path.display()))?;
    writeln!(file, "{digest}").with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

fn normalize_cgroup_parent_option(raw: Option<&str>) -> Result<Option<String>> {
    let Some(raw) = raw else {
        return Ok(None);
    };

    let normalized = normalize_cgroup_parent(raw)?;
    Ok(Some(normalized.to_string_lossy().into_owned()))
}

fn normalize_no_cache_filters(filters: &[String]) -> Result<Vec<String>> {
    let mut normalized = Vec::new();
    for raw in filters {
        let value = raw.trim();
        if value.is_empty() {
            bail!("invalid --no-cache-filter value: empty input");
        }
        normalized.push(value.to_string());
    }
    Ok(normalized)
}

#[derive(Debug, Clone)]
struct ParsedTag {
    repository: String,
    ref_name: String,
    has_explicit_tag: bool,
}

fn has_explicit_tag(raw: &str) -> bool {
    let last_colon = raw.rfind(':');
    let last_slash = raw.rfind('/');
    match (last_colon, last_slash) {
        (Some(colon), Some(slash)) => colon > slash,
        (Some(_), None) => true,
        _ => false,
    }
}

fn parse_tag(raw: &str) -> Result<ParsedTag> {
    if raw.trim().is_empty() {
        bail!("invalid -t/--tag: empty value");
    }
    if raw.contains('@') {
        bail!("invalid -t/--tag `{raw}`: digest references are not supported");
    }

    let reference = raw
        .parse::<Reference>()
        .with_context(|| format!("invalid -t/--tag image reference: `{raw}`"))?;

    let repository = reference.repository().to_string();
    let has_explicit_tag = has_explicit_tag(raw);
    let explicit_tag = reference.tag().map(|v| v.to_string());
    let ref_name = explicit_tag.unwrap_or_else(|| "latest".to_string());

    Ok(ParsedTag {
        repository,
        ref_name,
        has_explicit_tag,
    })
}

fn parse_tags(tags: &[String]) -> Result<Vec<ParsedTag>> {
    tags.iter().map(|tag| parse_tag(tag)).collect()
}

fn normalize_output_name(name: &str) -> String {
    let mut normalized = String::with_capacity(name.len());
    let mut prev_dash = false;

    for ch in name.chars() {
        let is_valid = ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_');
        let mapped = if is_valid { ch } else { '-' };
        if mapped == '-' {
            if prev_dash {
                continue;
            }
            prev_dash = true;
        } else {
            prev_dash = false;
        }
        normalized.push(mapped);
    }

    let normalized = normalized.trim_matches('-').to_string();
    if normalized.is_empty() {
        "image".to_string()
    } else {
        normalized
    }
}

fn derive_output_name(parsed_tags: &[ParsedTag], rng: impl FnOnce() -> String) -> String {
    if let Some(primary) = parsed_tags.first() {
        let repo_basename = primary
            .repository
            .rsplit('/')
            .next()
            .unwrap_or(primary.repository.as_str());
        let seed = if primary.has_explicit_tag {
            format!("{repo_basename}-{}", primary.ref_name)
        } else {
            repo_basename.to_string()
        };
        return normalize_output_name(&seed);
    }

    normalize_output_name(&rng())
}

fn unique_ref_names(parsed_tags: &[ParsedTag]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut uniq = Vec::new();
    for tag in parsed_tags {
        if seen.insert(tag.ref_name.clone()) {
            uniq.push(tag.ref_name.clone());
        }
    }
    uniq
}

fn normalize_push_reference(raw_tag: &str) -> String {
    if has_explicit_tag(raw_tag) {
        raw_tag.to_string()
    } else {
        format!("{raw_tag}:latest")
    }
}

pub fn build_image(build_args: &BuildArgs) -> Result<()> {
    if build_args.push && build_args.tags.is_empty() {
        bail!("--push requires at least one -t/--tag");
    }

    let build_started_at = Instant::now();
    let dockerfile_path = resolve_dockerfile_path(build_args)?;
    let dockerfile = parse_dockerfile(&dockerfile_path)?;
    let cli_build_args = parse_key_value_options(&build_args.build_args, "--build-arg")?;
    let cli_labels = parse_key_value_options(&build_args.labels, "--label")?;
    let cli_annotations = parse_key_value_options(&build_args.annotations, "--annotation")?;
    let no_cache_filters = normalize_no_cache_filters(&build_args.no_cache_filter)?;
    let cgroup_parent = normalize_cgroup_parent_option(build_args.cgroup_parent.as_deref())?;

    let output_dir = build_args
        .output_dir
        .as_ref()
        .map(|dir| dir.trim_end_matches('/').to_string())
        .unwrap_or_else(|| ".".to_string());

    let context = build_args.context.clone();

    let parsed_tags = parse_tags(&build_args.tags)?;
    let output_name = derive_output_name(&parsed_tags, || {
        let rng = rand::rng();
        rng.sample_iter(&Alphanumeric)
            .take(10)
            .map(char::from)
            .collect::<String>()
    });
    let image_output_dir = PathBuf::from(format!("{output_dir}/{output_name}"));
    let ref_names = if parsed_tags.is_empty() {
        vec!["latest".to_string()]
    } else {
        unique_ref_names(&parsed_tags)
    };
    let preferred_ref_name = ref_names.first().cloned();

    if image_output_dir.exists() {
        fs::remove_dir_all(&image_output_dir)?;
    }
    fs::create_dir_all(&image_output_dir)?;

    let global_args = parse_global_args(&dockerfile);
    let metadata_build_args = cli_build_args.clone();

    let mut executor = Executor::new(
        dockerfile,
        context,
        image_output_dir.clone(),
        ref_names,
        cli_build_args,
        global_args,
        Arc::new(TarGzCompressor),
    );
    executor.libfuse(build_args.libfuse);
    executor.no_cache(build_args.no_cache);
    executor.target(build_args.target.clone());
    executor.output_options(build_args.quiet, build_args.progress);
    executor.cli_labels(cli_labels);
    executor.cli_annotations(cli_annotations);
    executor.runtime_options(
        build_args.add_hosts.clone(),
        build_args.shm_size,
        build_args.ulimits.clone(),
        build_args.network,
        cgroup_parent,
    );
    executor.no_cache_filter(no_cache_filters);
    executor.secrets(build_args.secrets.clone());
    executor.ssh(build_args.ssh.clone());

    executor.build_image()?;

    let image_digest = read_primary_image_digest(&image_output_dir, preferred_ref_name.as_deref())?;
    if let Some(iidfile) = build_args.iidfile.as_ref() {
        write_iidfile(iidfile, &image_digest)?;
    }
    if build_args.push {
        let mut pushed_tags = HashSet::new();
        for tag in &build_args.tags {
            let push_ref = normalize_push_reference(tag);
            if pushed_tags.insert(push_ref.clone()) {
                push_from_layout(push_ref, &image_output_dir, None)?;
            }
        }
    }
    if let Some(metadata_file) = &build_args.metadata_file {
        let metadata = BuildMetadata::new(
            build_args.tags.clone(),
            image_digest.clone(),
            image_digest,
            metadata_build_args,
            build_started_at.elapsed().as_millis(),
        );
        write_metadata_file(metadata_file, &metadata)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use crate::image::build_runtime::BuildNetworkMode;

    use super::{
        BuildArgs, BuildProgressMode, derive_output_name, has_explicit_tag,
        normalize_cgroup_parent_option, normalize_push_reference, parse_add_host_option,
        parse_dockerfile, parse_global_args, parse_key_value_options, parse_secret_option,
        parse_shm_size, parse_ssh_option, parse_tags, parse_ulimit_option,
        read_primary_image_digest, resolve_dockerfile_path, unique_ref_names,
    };
    use clap::Parser;
    use dockerfile_parser::{BreakableStringComponent, Dockerfile, Instruction, ShellOrExecExpr};

    #[test]
    fn test_dockerfile() {
        let build_args =
            BuildArgs::parse_from(vec!["rkforge", "-f", "example-Dockerfile", "-t", "image1"]);

        assert_eq!(build_args.file, Some(PathBuf::from("example-Dockerfile")));
        assert_eq!(build_args.tags, vec!["image1".to_string()]);
        let dockerfile = parse_dockerfile(PathBuf::from("example-Dockerfile")).unwrap();
        assert_eq!(dockerfile.instructions.len(), 4);
    }

    #[test]
    fn test_output_dir() {
        let build_args = BuildArgs::parse_from(vec![
            "rkforge",
            "-f",
            "example-Dockerfile",
            "-t",
            "repo/image1:latest",
            "-o",
            "output_dir",
        ]);

        let parsed_tags = parse_tags(&build_args.tags).unwrap();
        let output_name = derive_output_name(&parsed_tags, || "RANDOM".to_string());
        assert_eq!("image1-latest", output_name);

        let build_args = BuildArgs::parse_from(vec!["rkforge", "-f", "example-Dockerfile"]);

        let parsed_tags = parse_tags(&build_args.tags).unwrap();
        let output_name = derive_output_name(&parsed_tags, || "RANDOM".to_string());

        assert_eq!("RANDOM", output_name);
    }

    #[test]
    fn test_parse_multiple_tags() {
        let build_args = BuildArgs::parse_from(vec![
            "rkforge",
            "-f",
            "example-Dockerfile",
            "-t",
            "example.com/ns/app:v1",
            "-t",
            "ns/app:v2",
        ]);
        assert_eq!(
            build_args.tags,
            vec!["example.com/ns/app:v1".to_string(), "ns/app:v2".to_string()]
        );
    }

    #[test]
    fn test_parse_tag_variants() {
        let parsed = parse_tags(&[
            "nginx".to_string(),
            "nginx:v1".to_string(),
            "registry.io/ns/app:latest".to_string(),
        ])
        .unwrap();

        assert_eq!(parsed[0].ref_name, "latest");
        assert!(!parsed[0].has_explicit_tag);
        assert_eq!(parsed[0].repository, "library/nginx");

        assert_eq!(parsed[1].ref_name, "v1");
        assert!(parsed[1].has_explicit_tag);

        assert_eq!(parsed[2].repository, "ns/app");
        assert_eq!(parsed[2].ref_name, "latest");
    }

    #[test]
    fn test_parse_tag_invalid() {
        assert!(parse_tags(&["".to_string()]).is_err());
        assert!(parse_tags(&["nginx@sha256:abc123".to_string()]).is_err());
    }

    #[test]
    fn test_has_explicit_tag() {
        assert!(!has_explicit_tag("nginx"));
        assert!(!has_explicit_tag("localhost:5000/ns/app"));
        assert!(has_explicit_tag("nginx:v1"));
        assert!(has_explicit_tag("localhost:5000/ns/app:v1"));
    }

    #[test]
    fn test_normalize_push_reference() {
        assert_eq!(normalize_push_reference("repo/app"), "repo/app:latest");
        assert_eq!(normalize_push_reference("repo/app:v1"), "repo/app:v1");
        assert_eq!(
            normalize_push_reference("localhost:5000/repo/app"),
            "localhost:5000/repo/app:latest"
        );
    }

    #[test]
    fn test_unique_ref_names() {
        let parsed = parse_tags(&[
            "repo/a:latest".to_string(),
            "repo/b:latest".to_string(),
            "repo/c:v1".to_string(),
        ])
        .unwrap();
        assert_eq!(
            unique_ref_names(&parsed),
            vec!["latest".to_string(), "v1".to_string()]
        );
    }

    #[test]
    fn test_run_instruction() {
        let build_args =
            BuildArgs::parse_from(vec!["rkforge", "-f", "example-Dockerfile", "-t", "image1"]);

        assert_eq!(build_args.file, Some(PathBuf::from("example-Dockerfile")));
        let dockerfile = parse_dockerfile(PathBuf::from("example-Dockerfile")).unwrap();
        for instruction in dockerfile.instructions.iter() {
            if let Instruction::Run(run_instruction) = instruction {
                match &run_instruction.expr {
                    ShellOrExecExpr::Exec(exec) => {
                        assert_eq!(exec.as_str_vec().len(), 5);
                    }
                    ShellOrExecExpr::Shell(shell_expr) => {
                        let mut commands = vec![];
                        commands.extend(vec!["/bin/sh", "-c"]);
                        for component in shell_expr.components.iter() {
                            match component {
                                BreakableStringComponent::Comment(_) => {}
                                BreakableStringComponent::String(spanned_string) => {
                                    commands.push(spanned_string.content.as_str());
                                }
                            }
                        }
                        tracing::debug!("commands: {commands:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn test_parse_key_value_options() {
        let parsed = parse_key_value_options(
            &[
                "FOO=bar".to_string(),
                "HELLO=world".to_string(),
                "FOO=baz".to_string(),
            ],
            "--build-arg",
        )
        .unwrap();
        assert_eq!(parsed.get("FOO"), Some(&"baz".to_string()));
        assert_eq!(parsed.get("HELLO"), Some(&"world".to_string()));
    }

    #[test]
    fn test_parse_key_value_options_invalid() {
        assert!(parse_key_value_options(&["INVALID".to_string()], "--build-arg").is_err());
        assert!(parse_key_value_options(&["=bar".to_string()], "--label").is_err());
    }

    #[test]
    fn test_resolve_dockerfile_path_prefers_dockerfile() {
        let temp_dir = tempfile::tempdir().unwrap();
        fs::write(temp_dir.path().join("Dockerfile"), "FROM scratch\n").unwrap();
        fs::write(temp_dir.path().join("Containerfile"), "FROM alpine\n").unwrap();
        let context = temp_dir.path().display().to_string();
        let build_args = BuildArgs::parse_from(vec!["rkforge", context.as_str()]);

        let resolved = resolve_dockerfile_path(&build_args).unwrap();
        assert_eq!(resolved, temp_dir.path().join("Dockerfile"));
    }

    #[test]
    fn test_resolve_dockerfile_path_fallback_to_containerfile() {
        let temp_dir = tempfile::tempdir().unwrap();
        fs::write(temp_dir.path().join("Containerfile"), "FROM alpine\n").unwrap();
        let context = temp_dir.path().display().to_string();
        let build_args = BuildArgs::parse_from(vec!["rkforge", context.as_str()]);

        let resolved = resolve_dockerfile_path(&build_args).unwrap();
        assert_eq!(resolved, temp_dir.path().join("Containerfile"));
    }

    #[test]
    fn test_resolve_dockerfile_path_missing_files_returns_error() {
        let temp_dir = tempfile::tempdir().unwrap();
        let context = temp_dir.path().display().to_string();
        let build_args = BuildArgs::parse_from(vec!["rkforge", context.as_str()]);

        assert!(resolve_dockerfile_path(&build_args).is_err());
    }

    #[test]
    fn test_resolve_dockerfile_path_file_flag_override() {
        let temp_dir = tempfile::tempdir().unwrap();
        fs::write(temp_dir.path().join("Dockerfile"), "FROM scratch\n").unwrap();
        let custom_file = temp_dir.path().join("Custom.Dockerfile");
        fs::write(&custom_file, "FROM alpine\n").unwrap();

        let context = temp_dir.path().display().to_string();
        let file = custom_file.display().to_string();
        let build_args =
            BuildArgs::parse_from(vec!["rkforge", "-f", file.as_str(), context.as_str()]);

        let resolved = resolve_dockerfile_path(&build_args).unwrap();
        assert_eq!(resolved, custom_file);
    }

    #[test]
    fn test_parse_global_args_collects_only_global_defaults() {
        let dockerfile = Dockerfile::parse(
            r#"
ARG BASE=ubuntu
ARG HTTP_PROXY
FROM ${BASE}
"#,
        )
        .unwrap();

        let global_args = parse_global_args(&dockerfile);

        assert_eq!(
            global_args.get("BASE").and_then(|value| value.as_deref()),
            Some("ubuntu")
        );
        assert_eq!(global_args.get("HTTP_PROXY"), Some(&None));
        assert!(!global_args.contains_key("NEW_ARG"));
    }

    #[test]
    fn test_parse_1() {
        let build_args = BuildArgs::parse_from(vec![
            "rkforge",
            "-f",
            "example-Dockerfile",
            "--target",
            "builder",
            "--build-arg",
            "FOO=bar",
            "--no-cache",
            "-q",
            "--iidfile",
            "/tmp/iid.txt",
            "--label",
            "a=b",
            "--progress",
            "plain",
            "--add-host",
            "mirror.local:10.0.0.2",
            "--shm-size",
            "64m",
            "--ulimit",
            "nofile=1024:2048",
            "--push",
            "--metadata-file",
            "/tmp/metadata.json",
            "--annotation",
            "org.opencontainers.image.source=https://example.com/repo",
            "--network",
            "host",
            "--cgroup-parent",
            "rkforge/build",
            "--no-cache-filter",
            "builder",
            ".",
        ]);

        assert_eq!(build_args.target, Some("builder".to_string()));
        assert_eq!(build_args.build_args, vec!["FOO=bar".to_string()]);
        assert!(build_args.no_cache);
        assert!(build_args.quiet);
        assert_eq!(build_args.iidfile, Some(PathBuf::from("/tmp/iid.txt")));
        assert_eq!(build_args.labels, vec!["a=b".to_string()]);
        assert_eq!(build_args.progress, BuildProgressMode::Plain);
        assert_eq!(build_args.add_hosts.len(), 1);
        assert_eq!(build_args.shm_size, Some(64 * 1024 * 1024));
        assert_eq!(build_args.ulimits.len(), 1);
        assert!(build_args.push);
        assert_eq!(
            build_args.metadata_file,
            Some(PathBuf::from("/tmp/metadata.json"))
        );
        assert_eq!(
            build_args.annotations,
            vec!["org.opencontainers.image.source=https://example.com/repo".to_string()]
        );
        assert_eq!(build_args.network, BuildNetworkMode::Host);
        assert_eq!(build_args.cgroup_parent, Some("rkforge/build".to_string()));
        assert_eq!(build_args.no_cache_filter, vec!["builder".to_string()]);
    }

    #[test]
    fn test_parse_add_host_option() {
        let host = parse_add_host_option("example.local:127.0.0.1").unwrap();
        assert_eq!(host.host, "example.local");
        assert_eq!(host.ip.to_string(), "127.0.0.1");
        assert!(parse_add_host_option("missing-ip").is_err());
        assert!(parse_add_host_option(":127.0.0.1").is_err());
    }

    #[test]
    fn test_parse_shm_size() {
        assert_eq!(parse_shm_size("64m").unwrap(), 64 * 1024 * 1024);
        assert_eq!(parse_shm_size("1g").unwrap(), 1024 * 1024 * 1024);
        assert_eq!(parse_shm_size("1024").unwrap(), 1024);
        assert!(parse_shm_size("0").is_err());
        assert!(parse_shm_size("10x").is_err());
    }

    #[test]
    fn test_parse_ulimit_option() {
        let parsed = parse_ulimit_option("nofile=1024:2048").unwrap();
        assert_eq!(parsed.resource.as_name(), "nofile");
        assert!(parse_ulimit_option("nofile=2048:1024").is_err());
        assert!(parse_ulimit_option("foo=1:2").is_err());
        assert!(parse_ulimit_option("nofile=1").is_err());
    }

    #[test]
    fn test_parse_no_cache_filter_option() {
        let build_args = BuildArgs::parse_from(vec![
            "rkforge",
            "--no-cache-filter",
            "builder",
            "--no-cache-filter",
            "1",
            ".",
        ]);
        assert_eq!(
            build_args.no_cache_filter,
            vec!["builder".to_string(), "1".to_string()]
        );
    }

    #[test]
    fn test_parse_network_and_cgroup_parent() {
        let build_args = BuildArgs::parse_from(vec![
            "rkforge",
            "--network",
            "none",
            "--cgroup-parent",
            "rkforge/build",
            ".",
        ]);
        assert_eq!(build_args.network, BuildNetworkMode::None);
        assert_eq!(build_args.cgroup_parent, Some("rkforge/build".to_string()));
    }

    #[test]
    fn test_normalize_cgroup_parent_option() {
        let normalized = normalize_cgroup_parent_option(Some("./rkforge/build")).unwrap();
        assert_eq!(normalized, Some("rkforge/build".to_string()));
        assert!(normalize_cgroup_parent_option(Some("../rkforge")).is_err());
    }

    #[test]
    fn test_read_primary_image_digest_prefers_ref_name() {
        let temp_dir = tempfile::tempdir().unwrap();
        let index_path = temp_dir.path().join("index.json");
        let index_json = r#"
{
  "schemaVersion": 2,
  "mediaType": "application/vnd.oci.image.index.v1+json",
  "manifests": [
    {
      "mediaType": "application/vnd.oci.image.manifest.v1+json",
      "digest": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      "size": 123,
      "annotations": {
        "org.opencontainers.image.ref.name": "v1"
      }
    },
    {
      "mediaType": "application/vnd.oci.image.manifest.v1+json",
      "digest": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
      "size": 456,
      "annotations": {
        "org.opencontainers.image.ref.name": "latest"
      }
    }
  ]
}
"#;
        fs::write(&index_path, index_json).unwrap();

        let digest = read_primary_image_digest(temp_dir.path(), Some("latest")).unwrap();
        assert_eq!(
            digest,
            "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        );
    }

    #[test]
    fn test_read_primary_image_digest_fallback_first() {
        let temp_dir = tempfile::tempdir().unwrap();
        let index_path = temp_dir.path().join("index.json");
        let index_json = r#"
{
  "schemaVersion": 2,
  "mediaType": "application/vnd.oci.image.index.v1+json",
  "manifests": [
    {
      "mediaType": "application/vnd.oci.image.manifest.v1+json",
      "digest": "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
      "size": 111
    }
  ]
}
"#;
        fs::write(&index_path, index_json).unwrap();

        let digest = read_primary_image_digest(temp_dir.path(), Some("missing")).unwrap();
        assert_eq!(
            digest,
            "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
        );
    }

    #[test]
    fn test_parse_secret_option_with_src() {
        let temp_dir = tempfile::tempdir().unwrap();
        let secret_file = temp_dir.path().join("my_secret");
        fs::write(&secret_file, b"supersecret").unwrap();

        let raw = format!("id=mysecret,src={}", secret_file.display());
        let parsed = parse_secret_option(&raw).unwrap();
        assert_eq!(parsed.id, "mysecret");
        assert_eq!(parsed.src, secret_file.canonicalize().unwrap());
    }

    #[test]
    fn test_parse_secret_option_default_src() {
        let temp_dir = tempfile::tempdir().unwrap();
        let secret_file = temp_dir.path().join("dbpass");
        fs::write(&secret_file, b"p@ss").unwrap();

        let raw = format!("id=dbpass,src={}", secret_file.display());
        let parsed = parse_secret_option(&raw).unwrap();
        assert_eq!(parsed.id, "dbpass");
        assert_eq!(parsed.src, secret_file.canonicalize().unwrap());
    }

    #[test]
    fn test_parse_secret_option_missing_id() {
        assert!(parse_secret_option("src=/tmp/x").is_err());
    }

    #[test]
    fn test_parse_secret_option_missing_file() {
        assert!(parse_secret_option("id=mysecret,src=/nonexistent/path").is_err());
    }

    #[test]
    fn test_parse_secret_option_unknown_key() {
        assert!(parse_secret_option("id=mysecret,foo=bar").is_err());
    }

    fn create_unix_socket(path: &std::path::Path) {
        use std::os::unix::net::UnixListener;
        let _ = UnixListener::bind(path).unwrap();
    }

    #[test]
    fn test_parse_ssh_option_with_socket() {
        let temp_dir = tempfile::tempdir().unwrap();
        let sock = temp_dir.path().join("agent.sock");
        create_unix_socket(&sock);

        let raw = format!("default={}", sock.display());
        let parsed = parse_ssh_option(&raw).unwrap();
        assert_eq!(parsed.id, "default");
        assert_eq!(parsed.socket_path, sock);
    }

    #[test]
    fn test_parse_ssh_option_missing_socket() {
        assert!(parse_ssh_option("default=/nonexistent/socket").is_err());
    }

    #[test]
    fn test_parse_ssh_option_empty_id() {
        assert!(parse_ssh_option("").is_err());
        assert!(parse_ssh_option("=/tmp/x").is_err());
    }

    #[test]
    fn test_parse_ssh_option_named_agent() {
        let temp_dir = tempfile::tempdir().unwrap();
        let sock = temp_dir.path().join("github.sock");
        create_unix_socket(&sock);

        let raw = format!("github={}", sock.display());
        let parsed = parse_ssh_option(&raw).unwrap();
        assert_eq!(parsed.id, "github");
        assert_eq!(parsed.socket_path, sock);
    }

    #[test]
    fn test_parse_ssh_option_rejects_regular_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let not_a_socket = temp_dir.path().join("regular.txt");
        fs::write(&not_a_socket, b"hello").unwrap();

        let raw = format!("default={}", not_a_socket.display());
        let err = parse_ssh_option(&raw).unwrap_err();
        assert!(err.contains("not a unix socket"), "error was: {err}");
    }

    #[test]
    fn test_parse_secret_option_path_traversal() {
        assert!(parse_secret_option("id=../etc/passwd,src=/etc/hostname").is_err());
        assert!(parse_secret_option("id=foo/../../etc/shadow,src=/etc/hostname").is_err());
        assert!(parse_secret_option("id=sub/file,src=/etc/hostname").is_err());
    }

    #[test]
    fn test_parse_1_with_secret_and_ssh() {
        let temp_dir = tempfile::tempdir().unwrap();
        let secret_file = temp_dir.path().join("token");
        fs::write(&secret_file, b"secret_token").unwrap();
        let ssh_sock = temp_dir.path().join("agent.sock");
        create_unix_socket(&ssh_sock);

        let secret_arg = format!("id=mytoken,src={}", secret_file.display());
        let ssh_arg = format!("default={}", ssh_sock.display());

        let build_args = BuildArgs::parse_from(vec![
            "rkforge",
            "-f",
            "example-Dockerfile",
            "--secret",
            &secret_arg,
            "--ssh",
            &ssh_arg,
            ".",
        ]);

        assert_eq!(build_args.secrets.len(), 1);
        assert_eq!(build_args.secrets[0].id, "mytoken");
        assert_eq!(build_args.ssh.len(), 1);
        assert_eq!(build_args.ssh[0].id, "default");
    }
}
