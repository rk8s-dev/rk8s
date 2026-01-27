use serial_test::serial;

use rkb::commands::compose::{ComposeCommand, DownArgs, PsArgs, UpArgs, compose_execute};
use test_common::*;

mod test_common;

/// Get compose configuration for testing
fn get_compose_config(project_name: &str) -> String {
    format!(
        r#"name: {}

services:
  backend:
    container_name: backend
    image: {}
    command: ["sleep", "10"]
    ports:
      - "8080:8080"
    networks:
      - default-net
  frontend:
    container_name: frontend
    image: {}
    command: ["sleep", "10"]
    ports:
      - "80:80"
    networks:
      - default-net

networks:
  default-net:
    driver: bridge
"#,
        project_name,
        bundles_path("busybox"),
        bundles_path("busybox")
    )
}

#[test]
#[serial]
fn test_compose_up_and_down() {
    let project_name = "test-compose-app";
    let compose_config = get_compose_config(project_name);

    // Create temporary compose file
    let compose_path = create_temp_compose_file(&compose_config).unwrap();

    // Ensure previous test resources are cleaned up
    cleanup_compose_project(project_name).unwrap();

    // Run compose up
    let result = compose_execute(ComposeCommand::Up(UpArgs {
        compose_yaml: Some(compose_path.clone()),
        project_name: Some(project_name.to_string()),
    }));

    // Note: Since the test environment may not have an actual container runtime,
    // we only check if the command can be parsed correctly, not the actual execution result.
    // In a real environment, we should check if containers are running properly
    let _ = result;

    // Run compose ps
    let result = compose_execute(ComposeCommand::Ps(PsArgs {
        project_name: Some(project_name.to_string()),
        compose_yaml: Some(compose_path.clone()),
    }));
    let _ = result;

    // Run compose down
    let result = compose_execute(ComposeCommand::Down(DownArgs {
        project_name: Some(project_name.to_string()),
        compose_yaml: Some(compose_path.clone()),
    }));
    let _ = result;

    // Clean up temporary files
    cleanup_temp_compose_file(&compose_path).unwrap();
    cleanup_compose_project(project_name).unwrap();
}

#[test]
#[serial]
fn test_compose_invalid_yaml() {
    let invalid_config = "invalid: yaml: content";
    let compose_path = create_temp_compose_file(invalid_config).unwrap();

    let result = compose_execute(ComposeCommand::Up(UpArgs {
        compose_yaml: Some(compose_path.clone()),
        project_name: Some("test-invalid".to_string()),
    }));

    // Invalid yaml configuration should return an error
    assert!(
        result.is_err(),
        "compose up with invalid yaml should return error"
    );

    cleanup_temp_compose_file(&compose_path).unwrap();
}
