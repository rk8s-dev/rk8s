pub mod config;
pub mod context;
pub mod execute;
pub mod executor;
pub mod stage_executor;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::compressor::tar_gz_compressor::TarGzCompressor;
use crate::image::executor::Executor;
use anyhow::{Context, Result, bail};
use clap::Parser;
use dockerfile_parser::Dockerfile;
use oci_spec::distribution::Reference;
use rand::{Rng, distr::Alphanumeric};

pub static BLOBS: &str = "blobs/sha256";

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
    let ref_name = explicit_tag.clone().unwrap_or_else(|| "latest".to_string());

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
    if let Some(dockerfile_path) = build_args.file.as_ref() {
        let dockerfile = parse_dockerfile(dockerfile_path)?;

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

        if image_output_dir.exists() {
            fs::remove_dir_all(&image_output_dir)?;
        }
        fs::create_dir_all(&image_output_dir)?;

        let global_args = parse_global_args(&dockerfile);

        let mut executor = Executor::new(
            dockerfile,
            context,
            image_output_dir,
            ref_names,
            global_args,
            Arc::new(TarGzCompressor),
        );
        executor.libfuse(build_args.libfuse);

        executor.build_image()?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{
        BuildArgs, derive_output_name, has_explicit_tag, parse_dockerfile, parse_tags,
        unique_ref_names,
    };
    use clap::Parser;
    use dockerfile_parser::{BreakableStringComponent, Instruction, ShellOrExecExpr};

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
}
