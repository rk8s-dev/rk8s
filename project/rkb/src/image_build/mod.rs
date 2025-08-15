pub mod builder;
pub mod config;
pub mod executor;
pub mod stage_executor;

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use dockerfile_parser::Dockerfile;
use rand::{Rng, distr::Alphanumeric};

use crate::build_arg::BuildArgs;
use builder::Builder;
use config::BuildConfig;

fn parse_dockerfile<P: AsRef<Path>>(dockerfile_path: P) -> Result<Dockerfile> {
    let dockerfile_path = dockerfile_path.as_ref().to_path_buf();
    let dockerfile_content = fs::read_to_string(&dockerfile_path)
        .with_context(|| format!("Failed to read Dockerfile: {}", dockerfile_path.display()))?;
    let dockerfile = Dockerfile::parse(&dockerfile_content)
        .with_context(|| format!("Failed to parse Dockerfile: {}", dockerfile_path.display()))?;
    Ok(dockerfile)
}

pub fn build_image(build_args: &BuildArgs) -> Result<()> {
    let build_config = BuildConfig::default()
        .verbose(build_args.verbose)
        .libfuse(build_args.libfuse);

    if let Some(dockerfile_path) = build_args.file.as_ref() {
        let dockerfile = parse_dockerfile(dockerfile_path)?;

        let output_dir = build_args
            .output_dir
            .as_ref()
            .map(|dir| dir.trim_end_matches('/').to_string())
            .unwrap_or_else(|| ".".to_string());

        let tag = build_args.tag.clone().unwrap_or_else(|| {
            let rng = rand::rng();
            rng.sample_iter(&Alphanumeric)
                .take(10)
                .map(char::from)
                .collect::<String>()
        });

        let image_output_dir = PathBuf::from(format!("{output_dir}/{tag}"));

        if fs::metadata(&image_output_dir).is_ok() {
            fs::remove_dir_all(&image_output_dir).with_context(|| {
                format!(
                    "Failed to remove existing directory: {}",
                    image_output_dir.display()
                )
            })?;
        }
        fs::create_dir_all(&image_output_dir).with_context(|| {
            format!(
                "Failed to create output directory: {}",
                image_output_dir.display()
            )
        })?;

        let image_builder = Builder::new(dockerfile)
            .config(build_config)
            .image_output_dir(image_output_dir);
        image_builder.build_image()?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use clap::Parser;
    use dockerfile_parser::{BreakableStringComponent, Instruction, ShellOrExecExpr};
    use rand::{Rng, distr::Alphanumeric};

    use super::BuildArgs;
    use super::parse_dockerfile;

    #[test]
    fn test_dockerfile() {
        let build_args =
            BuildArgs::parse_from(vec!["rkb", "-f", "example-Dockerfile", "-t", "image1"]);

        assert_eq!(build_args.file, Some(PathBuf::from("example-Dockerfile")));
        let dockerfile = parse_dockerfile(PathBuf::from("example-Dockerfile")).unwrap();
        assert_eq!(dockerfile.instructions.len(), 4);
    }

    #[test]
    fn test_output_dir() {
        let build_args = BuildArgs::parse_from(vec![
            "rkb",
            "-f",
            "example-Dockerfile",
            "-t",
            "image1",
            "-o",
            "output_dir",
        ]);

        let output_dir = build_args
            .output_dir
            .as_ref()
            .map(|dir| dir.trim_end_matches('/').to_string())
            .unwrap_or_else(|| ".".to_string());

        let tag = build_args.tag.clone().unwrap_or_else(|| {
            let rng = rand::rng();
            rng.sample_iter(&Alphanumeric)
                .take(10)
                .map(char::from)
                .collect::<String>()
        });

        let image_output_dir = PathBuf::from(&format!("{output_dir}/{tag}"));

        assert_eq!("output_dir/image1", image_output_dir.to_str().unwrap());

        let build_args = BuildArgs::parse_from(vec!["rkb", "-f", "example-Dockerfile"]);

        let output_dir = build_args
            .output_dir
            .as_ref()
            .map(|dir| dir.trim_end_matches('/').to_string())
            .unwrap_or_else(|| ".".to_string());

        let tag = build_args.tag.clone().unwrap_or_else(|| {
            let rng = rand::rng();
            rng.sample_iter(&Alphanumeric)
                .take(10)
                .map(char::from)
                .collect::<String>()
        });

        let image_output_dir = PathBuf::from(&format!("{output_dir}/{tag}"));

        assert!(image_output_dir.to_str().unwrap().starts_with("./"));
        assert_eq!(image_output_dir.to_str().unwrap().len(), 12);
    }

    #[test]
    fn test_run_instruction() {
        let build_args =
            BuildArgs::parse_from(vec!["rkb", "-f", "example-Dockerfile", "-t", "image1"]);

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
                        println!("commands: {commands:?}");
                    }
                }
            }
        }
    }
}
