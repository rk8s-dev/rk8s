pub mod config;
pub mod context;
pub mod execute;
pub mod executor;
pub mod stage_executor;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::compressor::tar_gz_compressor::TarGzCompressor;
use crate::image::executor::Executor;
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

    /// Build context. Defaults to the directory of the Dockerfile.
    #[arg(default_value = ".")]
    pub context: PathBuf,
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

pub fn build_image(build_args: &BuildArgs) -> Result<()> {
    let dockerfile_path = resolve_dockerfile_path(build_args)?;
    let dockerfile = parse_dockerfile(&dockerfile_path)?;
    let cli_build_args = parse_key_value_options(&build_args.build_args, "--build-arg")?;
    let cli_labels = parse_key_value_options(&build_args.labels, "--label")?;

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

    executor.build_image()?;

    let image_digest = read_primary_image_digest(&image_output_dir, preferred_ref_name.as_deref())?;
    if let Some(iidfile) = build_args.iidfile.as_ref() {
        write_iidfile(iidfile, &image_digest)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::{
        BuildArgs, BuildProgressMode, derive_output_name, has_explicit_tag, parse_dockerfile,
        parse_global_args, parse_key_value_options, parse_tags, read_primary_image_digest,
        resolve_dockerfile_path, unique_ref_names,
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
            ".",
        ]);

        assert_eq!(build_args.target, Some("builder".to_string()));
        assert_eq!(build_args.build_args, vec!["FOO=bar".to_string()]);
        assert!(build_args.no_cache);
        assert!(build_args.quiet);
        assert_eq!(build_args.iidfile, Some(PathBuf::from("/tmp/iid.txt")));
        assert_eq!(build_args.labels, vec!["a=b".to_string()]);
        assert_eq!(build_args.progress, BuildProgressMode::Plain);
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
}
