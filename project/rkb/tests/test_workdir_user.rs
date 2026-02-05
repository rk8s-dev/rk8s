//! Integration tests for WORKDIR and USER Dockerfile instructions.
//!
//! These tests verify that:
//! - WORKDIR instruction is correctly parsed
//! - USER instruction is correctly parsed
//! - Various formats are supported

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

/// Test WORKDIR instruction parsing with absolute path
#[test]
fn test_workdir_absolute_path() {
    let dockerfile_content = r#"
FROM ubuntu:latest
WORKDIR /app
RUN pwd
"#;

    let dockerfile = Dockerfile::parse(dockerfile_content).unwrap();

    let mut found_workdir = false;
    for instruction in dockerfile.instructions.iter() {
        if let Instruction::Misc(misc) = instruction {
            if misc.instruction.content.to_uppercase() == "WORKDIR" {
                found_workdir = true;
                let arg = extract_misc_argument(misc);
                assert_eq!(arg, "/app", "WORKDIR should be /app");
            }
        }
    }
    assert!(found_workdir, "WORKDIR instruction should be found");
}

/// Test WORKDIR instruction parsing with relative path
#[test]
fn test_workdir_relative_path() {
    let dockerfile_content = r#"
FROM ubuntu:latest
WORKDIR /app
WORKDIR subdir
RUN pwd
"#;

    let dockerfile = Dockerfile::parse(dockerfile_content).unwrap();

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

    assert_eq!(workdirs.len(), 2, "Should have 2 WORKDIR instructions");
    assert_eq!(workdirs[0], "/app");
    assert_eq!(workdirs[1], "subdir");
}

/// Test USER instruction parsing with username
#[test]
fn test_user_with_username() {
    let dockerfile_content = r#"
FROM ubuntu:latest
USER nobody
RUN whoami
"#;

    let dockerfile = Dockerfile::parse(dockerfile_content).unwrap();

    let mut found_user = false;
    for instruction in dockerfile.instructions.iter() {
        if let Instruction::Misc(misc) = instruction {
            if misc.instruction.content.to_uppercase() == "USER" {
                found_user = true;
                let arg = extract_misc_argument(misc);
                assert_eq!(arg, "nobody", "USER should be nobody");
            }
        }
    }
    assert!(found_user, "USER instruction should be found");
}

/// Test USER instruction parsing with uid
#[test]
fn test_user_with_uid() {
    let dockerfile_content = r#"
FROM ubuntu:latest
USER 1000
RUN id
"#;

    let dockerfile = Dockerfile::parse(dockerfile_content).unwrap();

    let mut found_user = false;
    for instruction in dockerfile.instructions.iter() {
        if let Instruction::Misc(misc) = instruction {
            if misc.instruction.content.to_uppercase() == "USER" {
                found_user = true;
                let arg = extract_misc_argument(misc);
                assert_eq!(arg, "1000", "USER should be 1000");
            }
        }
    }
    assert!(found_user, "USER instruction should be found");
}

/// Test USER instruction parsing with user:group format
#[test]
fn test_user_with_user_group() {
    let dockerfile_content = r#"
FROM ubuntu:latest
USER nobody:nogroup
RUN id
"#;

    let dockerfile = Dockerfile::parse(dockerfile_content).unwrap();

    let mut found_user = false;
    for instruction in dockerfile.instructions.iter() {
        if let Instruction::Misc(misc) = instruction {
            if misc.instruction.content.to_uppercase() == "USER" {
                found_user = true;
                let arg = extract_misc_argument(misc);
                assert_eq!(arg, "nobody:nogroup", "USER should be nobody:nogroup");
            }
        }
    }
    assert!(found_user, "USER instruction should be found");
}

/// Test USER instruction parsing with uid:gid format
#[test]
fn test_user_with_uid_gid() {
    let dockerfile_content = r#"
FROM ubuntu:latest
USER 1000:1000
RUN id
"#;

    let dockerfile = Dockerfile::parse(dockerfile_content).unwrap();

    let mut found_user = false;
    for instruction in dockerfile.instructions.iter() {
        if let Instruction::Misc(misc) = instruction {
            if misc.instruction.content.to_uppercase() == "USER" {
                found_user = true;
                let arg = extract_misc_argument(misc);
                assert_eq!(arg, "1000:1000", "USER should be 1000:1000");
            }
        }
    }
    assert!(found_user, "USER instruction should be found");
}

/// Test combined WORKDIR and USER instructions
#[test]
fn test_workdir_and_user_combined() {
    let dockerfile_content = r#"
FROM ubuntu:latest
WORKDIR /app
USER nobody
RUN echo "test"
WORKDIR /app/data
USER 1000:1000
RUN echo "another test"
"#;

    let dockerfile = Dockerfile::parse(dockerfile_content).unwrap();

    let workdir_count = dockerfile
        .instructions
        .iter()
        .filter(|i| {
            if let Instruction::Misc(misc) = i {
                misc.instruction.content.to_uppercase() == "WORKDIR"
            } else {
                false
            }
        })
        .count();

    let user_count = dockerfile
        .instructions
        .iter()
        .filter(|i| {
            if let Instruction::Misc(misc) = i {
                misc.instruction.content.to_uppercase() == "USER"
            } else {
                false
            }
        })
        .count();

    assert_eq!(workdir_count, 2, "Should have 2 WORKDIR instructions");
    assert_eq!(user_count, 2, "Should have 2 USER instructions");
}

/// Test COPY with WORKDIR (relative destination)
#[test]
fn test_copy_with_workdir() {
    let dockerfile_content = r#"
FROM ubuntu:latest
WORKDIR /app
COPY . .
COPY src/ ./src/
"#;

    let dockerfile = Dockerfile::parse(dockerfile_content).unwrap();

    let copy_count = dockerfile
        .instructions
        .iter()
        .filter(|i| matches!(i, Instruction::Copy(_)))
        .count();

    assert_eq!(copy_count, 2, "Should have 2 COPY instructions");
}

/// Test WORKDIR with path containing dots
#[test]
fn test_workdir_with_dots() {
    let dockerfile_content = r#"
FROM ubuntu:latest
WORKDIR /app/./subdir/../other
RUN pwd
"#;

    let dockerfile = Dockerfile::parse(dockerfile_content).unwrap();

    let mut found_workdir = false;
    for instruction in dockerfile.instructions.iter() {
        if let Instruction::Misc(misc) = instruction {
            if misc.instruction.content.to_uppercase() == "WORKDIR" {
                found_workdir = true;
                let arg = extract_misc_argument(misc);
                // The parser should preserve the original path
                assert_eq!(arg, "/app/./subdir/../other");
            }
        }
    }
    assert!(found_workdir, "WORKDIR instruction should be found");
}

/// Test WORKDIR with trailing slash
#[test]
fn test_workdir_trailing_slash() {
    let dockerfile_content = r#"
FROM ubuntu:latest
WORKDIR /app/
RUN pwd
"#;

    let dockerfile = Dockerfile::parse(dockerfile_content).unwrap();

    let mut found_workdir = false;
    for instruction in dockerfile.instructions.iter() {
        if let Instruction::Misc(misc) = instruction {
            if misc.instruction.content.to_uppercase() == "WORKDIR" {
                found_workdir = true;
                let arg = extract_misc_argument(misc);
                assert_eq!(arg, "/app/");
            }
        }
    }
    assert!(found_workdir, "WORKDIR instruction should be found");
}

/// Test USER with mixed uid:groupname format
#[test]
fn test_user_uid_groupname() {
    let dockerfile_content = r#"
FROM ubuntu:latest
USER 1000:nogroup
RUN id
"#;

    let dockerfile = Dockerfile::parse(dockerfile_content).unwrap();

    let mut found_user = false;
    for instruction in dockerfile.instructions.iter() {
        if let Instruction::Misc(misc) = instruction {
            if misc.instruction.content.to_uppercase() == "USER" {
                found_user = true;
                let arg = extract_misc_argument(misc);
                assert_eq!(arg, "1000:nogroup");
            }
        }
    }
    assert!(found_user, "USER instruction should be found");
}

/// Test USER with username:gid format
#[test]
fn test_user_username_gid() {
    let dockerfile_content = r#"
FROM ubuntu:latest
USER nobody:1000
RUN id
"#;

    let dockerfile = Dockerfile::parse(dockerfile_content).unwrap();

    let mut found_user = false;
    for instruction in dockerfile.instructions.iter() {
        if let Instruction::Misc(misc) = instruction {
            if misc.instruction.content.to_uppercase() == "USER" {
                found_user = true;
                let arg = extract_misc_argument(misc);
                assert_eq!(arg, "nobody:1000");
            }
        }
    }
    assert!(found_user, "USER instruction should be found");
}
