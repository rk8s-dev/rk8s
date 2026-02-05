//! Integration tests for VOLUME and STOPSIGNAL Dockerfile instructions.
//!
//! These tests verify that:
//! - VOLUME instruction is correctly parsed (single path, multiple paths, JSON array format)
//! - STOPSIGNAL instruction is correctly parsed (signal name and number formats)

use dockerfile_parser::{Dockerfile, Instruction};

/// Helper function to extract argument from Misc instruction
fn extract_misc_argument(misc: &dockerfile_parser::MiscInstruction) -> String {
    use dockerfile_parser::BreakableStringComponent;
    let mut result = String::new();
    for component in misc.arguments.components.iter() {
        match component {
            BreakableStringComponent::Comment(_) => {}
            BreakableStringComponent::String(spanned_string) => {
                result.push_str(&spanned_string.content);
            }
        }
    }
    result.trim().to_string()
}

// ============================================================================
// VOLUME instruction tests
// ============================================================================

/// Test VOLUME instruction parsing with single path
#[test]
fn test_volume_single_path() {
    let dockerfile_content = r#"
FROM ubuntu:latest
VOLUME /data
RUN echo "test"
"#;

    let dockerfile = Dockerfile::parse(dockerfile_content).unwrap();

    let mut found_volume = false;
    for instruction in dockerfile.instructions.iter() {
        if let Instruction::Misc(misc) = instruction {
            if misc.instruction.content.to_uppercase() == "VOLUME" {
                found_volume = true;
                let arg = extract_misc_argument(misc);
                assert_eq!(arg, "/data", "VOLUME should be /data");
            }
        }
    }
    assert!(found_volume, "VOLUME instruction should be found");
}

/// Test VOLUME instruction parsing with multiple space-separated paths
#[test]
fn test_volume_multiple_paths() {
    let dockerfile_content = r#"
FROM ubuntu:latest
VOLUME /data /logs /config
RUN echo "test"
"#;

    let dockerfile = Dockerfile::parse(dockerfile_content).unwrap();

    let mut found_volume = false;
    for instruction in dockerfile.instructions.iter() {
        if let Instruction::Misc(misc) = instruction {
            if misc.instruction.content.to_uppercase() == "VOLUME" {
                found_volume = true;
                let arg = extract_misc_argument(misc);
                // The parser should preserve the original format
                assert!(
                    arg.contains("/data") && arg.contains("/logs") && arg.contains("/config"),
                    "VOLUME should contain all paths"
                );
            }
        }
    }
    assert!(found_volume, "VOLUME instruction should be found");
}

/// Test VOLUME instruction parsing with JSON array format
#[test]
fn test_volume_json_array() {
    let dockerfile_content = r#"
FROM ubuntu:latest
VOLUME ["/data", "/logs"]
RUN echo "test"
"#;

    let dockerfile = Dockerfile::parse(dockerfile_content).unwrap();

    let mut found_volume = false;
    for instruction in dockerfile.instructions.iter() {
        if let Instruction::Misc(misc) = instruction {
            if misc.instruction.content.to_uppercase() == "VOLUME" {
                found_volume = true;
                let arg = extract_misc_argument(misc);
                // JSON format should be preserved
                assert!(arg.starts_with('['), "VOLUME should be in JSON array format");
                assert!(arg.contains("/data"), "VOLUME should contain /data");
                assert!(arg.contains("/logs"), "VOLUME should contain /logs");
            }
        }
    }
    assert!(found_volume, "VOLUME instruction should be found");
}

/// Test multiple VOLUME instructions
#[test]
fn test_volume_multiple_instructions() {
    let dockerfile_content = r#"
FROM ubuntu:latest
VOLUME /data
VOLUME /logs
VOLUME /config
RUN echo "test"
"#;

    let dockerfile = Dockerfile::parse(dockerfile_content).unwrap();

    let volume_count = dockerfile
        .instructions
        .iter()
        .filter(|i| {
            if let Instruction::Misc(misc) = i {
                misc.instruction.content.to_uppercase() == "VOLUME"
            } else {
                false
            }
        })
        .count();

    assert_eq!(volume_count, 3, "Should have 3 VOLUME instructions");
}

/// Test VOLUME with complex path
#[test]
fn test_volume_complex_path() {
    let dockerfile_content = r#"
FROM ubuntu:latest
VOLUME /var/lib/mysql
RUN echo "test"
"#;

    let dockerfile = Dockerfile::parse(dockerfile_content).unwrap();

    let mut found_volume = false;
    for instruction in dockerfile.instructions.iter() {
        if let Instruction::Misc(misc) = instruction {
            if misc.instruction.content.to_uppercase() == "VOLUME" {
                found_volume = true;
                let arg = extract_misc_argument(misc);
                assert_eq!(arg, "/var/lib/mysql", "VOLUME should be /var/lib/mysql");
            }
        }
    }
    assert!(found_volume, "VOLUME instruction should be found");
}

// ============================================================================
// STOPSIGNAL instruction tests
// ============================================================================

/// Test STOPSIGNAL instruction parsing with signal name
#[test]
fn test_stopsignal_name() {
    let dockerfile_content = r#"
FROM ubuntu:latest
STOPSIGNAL SIGTERM
RUN echo "test"
"#;

    let dockerfile = Dockerfile::parse(dockerfile_content).unwrap();

    let mut found_stopsignal = false;
    for instruction in dockerfile.instructions.iter() {
        if let Instruction::Misc(misc) = instruction {
            if misc.instruction.content.to_uppercase() == "STOPSIGNAL" {
                found_stopsignal = true;
                let arg = extract_misc_argument(misc);
                assert_eq!(arg, "SIGTERM", "STOPSIGNAL should be SIGTERM");
            }
        }
    }
    assert!(found_stopsignal, "STOPSIGNAL instruction should be found");
}

/// Test STOPSIGNAL instruction parsing with SIGKILL
#[test]
fn test_stopsignal_sigkill() {
    let dockerfile_content = r#"
FROM ubuntu:latest
STOPSIGNAL SIGKILL
RUN echo "test"
"#;

    let dockerfile = Dockerfile::parse(dockerfile_content).unwrap();

    let mut found_stopsignal = false;
    for instruction in dockerfile.instructions.iter() {
        if let Instruction::Misc(misc) = instruction {
            if misc.instruction.content.to_uppercase() == "STOPSIGNAL" {
                found_stopsignal = true;
                let arg = extract_misc_argument(misc);
                assert_eq!(arg, "SIGKILL", "STOPSIGNAL should be SIGKILL");
            }
        }
    }
    assert!(found_stopsignal, "STOPSIGNAL instruction should be found");
}

/// Test STOPSIGNAL instruction parsing with signal number
#[test]
fn test_stopsignal_number() {
    let dockerfile_content = r#"
FROM ubuntu:latest
STOPSIGNAL 9
RUN echo "test"
"#;

    let dockerfile = Dockerfile::parse(dockerfile_content).unwrap();

    let mut found_stopsignal = false;
    for instruction in dockerfile.instructions.iter() {
        if let Instruction::Misc(misc) = instruction {
            if misc.instruction.content.to_uppercase() == "STOPSIGNAL" {
                found_stopsignal = true;
                let arg = extract_misc_argument(misc);
                assert_eq!(arg, "9", "STOPSIGNAL should be 9");
            }
        }
    }
    assert!(found_stopsignal, "STOPSIGNAL instruction should be found");
}

/// Test STOPSIGNAL with SIGINT
#[test]
fn test_stopsignal_sigint() {
    let dockerfile_content = r#"
FROM ubuntu:latest
STOPSIGNAL SIGINT
RUN echo "test"
"#;

    let dockerfile = Dockerfile::parse(dockerfile_content).unwrap();

    let mut found_stopsignal = false;
    for instruction in dockerfile.instructions.iter() {
        if let Instruction::Misc(misc) = instruction {
            if misc.instruction.content.to_uppercase() == "STOPSIGNAL" {
                found_stopsignal = true;
                let arg = extract_misc_argument(misc);
                assert_eq!(arg, "SIGINT", "STOPSIGNAL should be SIGINT");
            }
        }
    }
    assert!(found_stopsignal, "STOPSIGNAL instruction should be found");
}

/// Test STOPSIGNAL with SIGRTMIN+3 format
#[test]
fn test_stopsignal_sigrtmin_plus() {
    let dockerfile_content = r#"
FROM ubuntu:latest
STOPSIGNAL SIGRTMIN+3
RUN echo "test"
"#;

    let dockerfile = Dockerfile::parse(dockerfile_content).unwrap();

    let mut found_stopsignal = false;
    for instruction in dockerfile.instructions.iter() {
        if let Instruction::Misc(misc) = instruction {
            if misc.instruction.content.to_uppercase() == "STOPSIGNAL" {
                found_stopsignal = true;
                let arg = extract_misc_argument(misc);
                assert_eq!(arg, "SIGRTMIN+3", "STOPSIGNAL should be SIGRTMIN+3");
            }
        }
    }
    assert!(found_stopsignal, "STOPSIGNAL instruction should be found");
}

// ============================================================================
// Combined tests
// ============================================================================

/// Test combined VOLUME and STOPSIGNAL instructions
#[test]
fn test_volume_and_stopsignal_combined() {
    let dockerfile_content = r#"
FROM ubuntu:latest
VOLUME /data
STOPSIGNAL SIGTERM
RUN echo "test"
"#;

    let dockerfile = Dockerfile::parse(dockerfile_content).unwrap();

    let volume_count = dockerfile
        .instructions
        .iter()
        .filter(|i| {
            if let Instruction::Misc(misc) = i {
                misc.instruction.content.to_uppercase() == "VOLUME"
            } else {
                false
            }
        })
        .count();

    let stopsignal_count = dockerfile
        .instructions
        .iter()
        .filter(|i| {
            if let Instruction::Misc(misc) = i {
                misc.instruction.content.to_uppercase() == "STOPSIGNAL"
            } else {
                false
            }
        })
        .count();

    assert_eq!(volume_count, 1, "Should have 1 VOLUME instruction");
    assert_eq!(stopsignal_count, 1, "Should have 1 STOPSIGNAL instruction");
}

/// Test real-world Dockerfile pattern (like nginx)
#[test]
fn test_nginx_like_dockerfile() {
    let dockerfile_content = r#"
FROM debian:bookworm-slim
LABEL maintainer="NGINX Docker Maintainers"
RUN apt-get update && apt-get install -y nginx
VOLUME /var/cache/nginx
STOPSIGNAL SIGQUIT
EXPOSE 80
CMD ["nginx", "-g", "daemon off;"]
"#;

    let dockerfile = Dockerfile::parse(dockerfile_content).unwrap();

    let mut found_volume = false;
    let mut found_stopsignal = false;

    for instruction in dockerfile.instructions.iter() {
        if let Instruction::Misc(misc) = instruction {
            match misc.instruction.content.to_uppercase().as_str() {
                "VOLUME" => {
                    found_volume = true;
                    let arg = extract_misc_argument(misc);
                    assert_eq!(arg, "/var/cache/nginx");
                }
                "STOPSIGNAL" => {
                    found_stopsignal = true;
                    let arg = extract_misc_argument(misc);
                    assert_eq!(arg, "SIGQUIT");
                }
                _ => {}
            }
        }
    }

    assert!(found_volume, "VOLUME instruction should be found");
    assert!(found_stopsignal, "STOPSIGNAL instruction should be found");
}

/// Test real-world Dockerfile pattern (like MySQL)
#[test]
fn test_mysql_like_dockerfile() {
    let dockerfile_content = r#"
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y mysql-server
VOLUME /var/lib/mysql
STOPSIGNAL SIGTERM
EXPOSE 3306
CMD ["mysqld"]
"#;

    let dockerfile = Dockerfile::parse(dockerfile_content).unwrap();

    let mut found_volume = false;
    let mut found_stopsignal = false;

    for instruction in dockerfile.instructions.iter() {
        if let Instruction::Misc(misc) = instruction {
            match misc.instruction.content.to_uppercase().as_str() {
                "VOLUME" => {
                    found_volume = true;
                    let arg = extract_misc_argument(misc);
                    assert_eq!(arg, "/var/lib/mysql");
                }
                "STOPSIGNAL" => {
                    found_stopsignal = true;
                    let arg = extract_misc_argument(misc);
                    assert_eq!(arg, "SIGTERM");
                }
                _ => {}
            }
        }
    }

    assert!(found_volume, "VOLUME instruction should be found");
    assert!(found_stopsignal, "STOPSIGNAL instruction should be found");
}

/// Test real-world Dockerfile pattern (like Redis with multiple volumes)
#[test]
fn test_redis_like_dockerfile() {
    let dockerfile_content = r#"
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y redis-server
VOLUME /data
WORKDIR /data
STOPSIGNAL SIGTERM
EXPOSE 6379
CMD ["redis-server"]
"#;

    let dockerfile = Dockerfile::parse(dockerfile_content).unwrap();

    let mut found_volume = false;
    let mut found_stopsignal = false;
    let mut found_workdir = false;

    for instruction in dockerfile.instructions.iter() {
        if let Instruction::Misc(misc) = instruction {
            match misc.instruction.content.to_uppercase().as_str() {
                "VOLUME" => {
                    found_volume = true;
                    let arg = extract_misc_argument(misc);
                    assert_eq!(arg, "/data");
                }
                "STOPSIGNAL" => {
                    found_stopsignal = true;
                    let arg = extract_misc_argument(misc);
                    assert_eq!(arg, "SIGTERM");
                }
                "WORKDIR" => {
                    found_workdir = true;
                    let arg = extract_misc_argument(misc);
                    assert_eq!(arg, "/data");
                }
                _ => {}
            }
        }
    }

    assert!(found_volume, "VOLUME instruction should be found");
    assert!(found_stopsignal, "STOPSIGNAL instruction should be found");
    assert!(found_workdir, "WORKDIR instruction should be found");
}

/// Test PostgreSQL-like Dockerfile with multiple volumes in JSON format
#[test]
fn test_postgres_like_dockerfile() {
    let dockerfile_content = r#"
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y postgresql
VOLUME ["/var/lib/postgresql/data", "/var/run/postgresql"]
STOPSIGNAL SIGINT
USER postgres
EXPOSE 5432
CMD ["postgres"]
"#;

    let dockerfile = Dockerfile::parse(dockerfile_content).unwrap();

    let mut found_volume = false;
    let mut found_stopsignal = false;
    let mut found_user = false;

    for instruction in dockerfile.instructions.iter() {
        if let Instruction::Misc(misc) = instruction {
            match misc.instruction.content.to_uppercase().as_str() {
                "VOLUME" => {
                    found_volume = true;
                    let arg = extract_misc_argument(misc);
                    assert!(arg.contains("/var/lib/postgresql/data"));
                    assert!(arg.contains("/var/run/postgresql"));
                }
                "STOPSIGNAL" => {
                    found_stopsignal = true;
                    let arg = extract_misc_argument(misc);
                    assert_eq!(arg, "SIGINT");
                }
                "USER" => {
                    found_user = true;
                    let arg = extract_misc_argument(misc);
                    assert_eq!(arg, "postgres");
                }
                _ => {}
            }
        }
    }

    assert!(found_volume, "VOLUME instruction should be found");
    assert!(found_stopsignal, "STOPSIGNAL instruction should be found");
    assert!(found_user, "USER instruction should be found");
}
