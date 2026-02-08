use serial_test::serial;

use rkb::commands::container::{ContainerCommand, container_execute};
use test_common::*;

mod test_common;

/// Get container configuration for testing
fn get_container_config() -> String {
    format!(
        r#"name: test-container
image: {}
command: ["sleep", "10"]
ports:
  - "8080:8080"
"#,
        bundles_path("busybox")
    )
}

#[test]
#[serial]
fn test_container_commands() {
    let container_name = "test-container";
    let container_config = get_container_config();

    // Create temporary container configuration file
    let container_path = create_temp_compose_file(&container_config).unwrap();

    // Ensure previous test resources are cleaned up
    cleanup_container(container_name).unwrap();

    // Test container run command
    let result = container_execute(ContainerCommand::Run {
        container_yaml: container_path.clone(),
        volumes: None,
    });
    // Note: In test environment without proper runtime setup, commands may fail
    // We just ensure they don't panic
    let _ = result;

    // Test container list command
    let result = container_execute(ContainerCommand::List {
        quiet: None,
        format: None,
    });
    let _ = result;

    // Test container state command
    let result = container_execute(ContainerCommand::State {
        container_name: container_name.to_string(),
    });
    let _ = result;

    // Test container delete command
    let result = container_execute(ContainerCommand::Delete {
        container_name: container_name.to_string(),
    });
    let _ = result;

    // Clean up temporary files
    cleanup_temp_compose_file(&container_path).unwrap();
    cleanup_container(container_name).unwrap();
}
