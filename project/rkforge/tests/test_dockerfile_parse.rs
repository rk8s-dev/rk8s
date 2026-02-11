//! Integration tests for Dockerfile instruction parsing.
//!
//! These tests verify that various Dockerfile instructions are correctly parsed
//! by the dockerfile_parser library, including:
//! - WORKDIR, USER, SHELL, EXPOSE, VOLUME, STOPSIGNAL
//!
//! Note: These are pure string parsing tests that don't build actual images.

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
// WORKDIR instruction tests
// ============================================================================

#[test]
fn test_workdir_absolute_path() {
    let dockerfile = Dockerfile::parse(
        r#"
FROM ubuntu:latest
WORKDIR /app
"#,
    )
    .unwrap();

    let workdir = dockerfile
        .instructions
        .iter()
        .find_map(|i| {
            if let Instruction::Misc(misc) = i {
                if misc.instruction.content.to_uppercase() == "WORKDIR" {
                    return Some(extract_misc_argument(misc));
                }
            }
            None
        })
        .unwrap();

    assert_eq!(workdir, "/app");
}

#[test]
fn test_workdir_relative_path() {
    let dockerfile = Dockerfile::parse(
        r#"
FROM ubuntu:latest
WORKDIR /app
WORKDIR subdir
"#,
    )
    .unwrap();

    let workdirs: Vec<String> = dockerfile
        .instructions
        .iter()
        .filter_map(|i| {
            if let Instruction::Misc(misc) = i {
                if misc.instruction.content.to_uppercase() == "WORKDIR" {
                    return Some(extract_misc_argument(misc));
                }
            }
            None
        })
        .collect();

    assert_eq!(workdirs, vec!["/app", "subdir"]);
}

// ============================================================================
// USER instruction tests
// ============================================================================

#[test]
fn test_user_formats() {
    let dockerfile = Dockerfile::parse(
        r#"
FROM ubuntu:latest
USER nobody
USER 1000
USER nobody:nogroup
USER 1000:1000
"#,
    )
    .unwrap();

    let users: Vec<String> = dockerfile
        .instructions
        .iter()
        .filter_map(|i| {
            if let Instruction::Misc(misc) = i {
                if misc.instruction.content.to_uppercase() == "USER" {
                    return Some(extract_misc_argument(misc));
                }
            }
            None
        })
        .collect();

    assert_eq!(users, vec!["nobody", "1000", "nobody:nogroup", "1000:1000"]);
}

// ============================================================================
// SHELL instruction tests
// ============================================================================

#[test]
fn test_shell_formats() {
    let dockerfile = Dockerfile::parse(
        r#"
FROM ubuntu:latest
SHELL ["/bin/bash", "-c"]
SHELL ["/bin/sh", "-c"]
"#,
    )
    .unwrap();

    let shells: Vec<String> = dockerfile
        .instructions
        .iter()
        .filter_map(|i| {
            if let Instruction::Misc(misc) = i {
                if misc.instruction.content.to_uppercase() == "SHELL" {
                    return Some(extract_misc_argument(misc));
                }
            }
            None
        })
        .collect();

    assert_eq!(shells.len(), 2);
    assert!(shells[0].contains("/bin/bash"));
    assert!(shells[1].contains("/bin/sh"));
}

// ============================================================================
// EXPOSE instruction tests
// ============================================================================

#[test]
fn test_expose_formats() {
    let dockerfile = Dockerfile::parse(
        r#"
FROM ubuntu:latest
EXPOSE 80
EXPOSE 443/tcp
EXPOSE 53/udp
EXPOSE 8080 9090
"#,
    )
    .unwrap();

    let exposes: Vec<String> = dockerfile
        .instructions
        .iter()
        .filter_map(|i| {
            if let Instruction::Misc(misc) = i {
                if misc.instruction.content.to_uppercase() == "EXPOSE" {
                    return Some(extract_misc_argument(misc));
                }
            }
            None
        })
        .collect();

    assert_eq!(exposes[0], "80");
    assert_eq!(exposes[1], "443/tcp");
    assert_eq!(exposes[2], "53/udp");
    assert!(exposes[3].contains("8080") && exposes[3].contains("9090"));
}

// ============================================================================
// VOLUME instruction tests
// ============================================================================

#[test]
fn test_volume_formats() {
    let dockerfile = Dockerfile::parse(
        r#"
FROM ubuntu:latest
VOLUME /data
VOLUME /logs /config
VOLUME ["/var/lib/mysql", "/var/run/mysqld"]
"#,
    )
    .unwrap();

    let volumes: Vec<String> = dockerfile
        .instructions
        .iter()
        .filter_map(|i| {
            if let Instruction::Misc(misc) = i {
                if misc.instruction.content.to_uppercase() == "VOLUME" {
                    return Some(extract_misc_argument(misc));
                }
            }
            None
        })
        .collect();

    assert_eq!(volumes[0], "/data");
    assert!(volumes[1].contains("/logs") && volumes[1].contains("/config"));
    assert!(volumes[2].starts_with('['));
    assert!(volumes[2].contains("/var/lib/mysql"));
    assert!(volumes[2].contains("/var/run/mysqld"));
}

// ============================================================================
// STOPSIGNAL instruction tests
// ============================================================================

#[test]
fn test_stopsignal_formats() {
    let dockerfile = Dockerfile::parse(
        r#"
FROM ubuntu:latest
STOPSIGNAL SIGTERM
STOPSIGNAL SIGKILL
STOPSIGNAL 9
STOPSIGNAL SIGINT
"#,
    )
    .unwrap();

    let signals: Vec<String> = dockerfile
        .instructions
        .iter()
        .filter_map(|i| {
            if let Instruction::Misc(misc) = i {
                if misc.instruction.content.to_uppercase() == "STOPSIGNAL" {
                    return Some(extract_misc_argument(misc));
                }
            }
            None
        })
        .collect();

    assert_eq!(signals, vec!["SIGTERM", "SIGKILL", "9", "SIGINT"]);
}

// ============================================================================
// Combined tests
// ============================================================================

#[test]
fn test_combined_instructions() {
    let dockerfile = Dockerfile::parse(
        r#"
FROM ubuntu:latest
WORKDIR /app
USER appuser
SHELL ["/bin/bash", "-c"]
EXPOSE 80 443
VOLUME /data
STOPSIGNAL SIGTERM
CMD ["./start.sh"]
"#,
    )
    .unwrap();

    let mut instructions_found = std::collections::HashSet::new();

    for instruction in dockerfile.instructions.iter() {
        if let Instruction::Misc(misc) = instruction {
            instructions_found.insert(misc.instruction.content.to_uppercase());
        }
    }

    assert!(instructions_found.contains("WORKDIR"));
    assert!(instructions_found.contains("USER"));
    assert!(instructions_found.contains("SHELL"));
    assert!(instructions_found.contains("EXPOSE"));
    assert!(instructions_found.contains("VOLUME"));
    assert!(instructions_found.contains("STOPSIGNAL"));
}
