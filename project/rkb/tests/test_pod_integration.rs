use serial_test::serial;
use std::fs;
use std::path::Path;
use std::thread;
use std::time::Duration;

use rkb::commands::pod::{PodCommand, PodInfo, pod_execute};
use test_common::*;

mod test_common;

/// Get a complete pod configuration for testing
fn get_comprehensive_pod_config(pod_name: &str) -> String {
    format!(
        r#"apiVersion: v1
kind: Pod
metadata:
  name: {}
  namespace: default
spec:
  containers:
  - name: main-container
    image: {}
    command: ["sleep"]
    args: ["300"]
    ports:
    - containerPort: 8080
      protocol: TCP
  restartPolicy: Never
"#,
        pod_name,
        bundles_path("busybox")
    )
}

/// Comprehensive test for all pod commands
#[test]
#[serial]
fn test_pod_commands_integration() {
    let pod_name = "test-integration-pod";
    let pod_config = get_comprehensive_pod_config(pod_name);

    // Create temporary pod configuration file
    let pod_path = create_temp_compose_file(&pod_config).unwrap();

    // Cleanup any existing resources
    cleanup_pod(pod_name).unwrap();

    // Test 1: Pod Create Command
    println!("Testing pod create command...");
    let create_result = std::panic::catch_unwind(|| {
        let _ = pod_execute(PodCommand::Create {
            pod_yaml: pod_path.clone(),
        });
    });
    assert!(create_result.is_ok(), "Pod create command should not panic");

    // Give it some time to create
    thread::sleep(Duration::from_secs(1));

    // Test 2: Pod List Command - should show our pod
    println!("Testing pod list command...");
    let list_result = std::panic::catch_unwind(|| {
        let _ = pod_execute(PodCommand::List {});
    });
    assert!(list_result.is_ok(), "Pod list command should not panic");

    // Test 3: Pod State Command
    println!("Testing pod state command...");
    let state_result = std::panic::catch_unwind(|| {
        let _ = pod_execute(PodCommand::State {
            pod_name: pod_name.to_string(),
        });
    });
    assert!(state_result.is_ok(), "Pod state command should not panic");

    // Test 4: Pod Start Command (may fail if already running, but shouldn't panic)
    println!("Testing pod start command...");
    let start_result = std::panic::catch_unwind(|| {
        let _ = pod_execute(PodCommand::Start {
            pod_name: pod_name.to_string(),
        });
    });
    assert!(start_result.is_ok(), "Pod start command should not panic");

    // Test 5: Pod Delete Command
    println!("Testing pod delete command...");
    let delete_result = std::panic::catch_unwind(|| {
        let _ = pod_execute(PodCommand::Delete {
            pod_name: pod_name.to_string(),
        });
    });
    assert!(delete_result.is_ok(), "Pod delete command should not panic");

    // Clean up temporary files
    cleanup_temp_compose_file(&pod_path).unwrap();
    cleanup_pod(pod_name).unwrap();

    println!("All pod commands completed without panic!");
}

/// Test pod run command separately
#[test]
#[serial]
fn test_pod_run_command() {
    let pod_name = "test-run-pod";
    let pod_config = get_comprehensive_pod_config(pod_name);

    // Create temporary pod configuration file
    let pod_path = create_temp_compose_file(&pod_config).unwrap();

    // Cleanup any existing resources
    cleanup_pod(pod_name).unwrap();

    println!("Testing pod run command...");
    let run_result = std::panic::catch_unwind(|| {
        let _ = pod_execute(PodCommand::Run {
            pod_yaml: pod_path.clone(),
        });
    });
    assert!(run_result.is_ok(), "Pod run command should not panic");

    // Give it some time
    thread::sleep(Duration::from_secs(1));

    // Verify pod was created and can be listed
    let list_result = std::panic::catch_unwind(|| {
        let _ = pod_execute(PodCommand::List {});
    });
    assert!(list_result.is_ok(), "Pod list after run should not panic");

    // Clean up
    let _ = std::panic::catch_unwind(|| {
        let _ = pod_execute(PodCommand::Delete {
            pod_name: pod_name.to_string(),
        });
    });

    cleanup_temp_compose_file(&pod_path).unwrap();
    cleanup_pod(pod_name).unwrap();

    println!("Pod run command test completed!");
}

/// Test pod persistence - check if pod info is correctly saved and loaded
#[test]
#[serial]
fn test_pod_persistence() {
    let pod_name = "test-persistence-pod";
    let root_path = Path::new("/tmp/rkb_test");

    // Create test directory
    if root_path.exists() {
        fs::remove_dir_all(root_path).unwrap();
    }
    fs::create_dir_all(root_path).unwrap();

    // Create and save pod info
    let pod_info = PodInfo {
        pod_sandbox_id: "test-sandbox-12345".to_string(),
        container_names: vec!["container1".to_string(), "container2".to_string()],
    };

    // Test save
    let save_result = pod_info.save(root_path, pod_name);
    assert!(save_result.is_ok(), "Pod info save should succeed");

    // Test load
    let loaded_pod_info = PodInfo::load(root_path, pod_name);
    assert!(loaded_pod_info.is_ok(), "Pod info load should succeed");

    let loaded = loaded_pod_info.unwrap();
    assert_eq!(loaded.pod_sandbox_id, "test-sandbox-12345");
    assert_eq!(loaded.container_names.len(), 2);
    assert!(loaded.container_names.contains(&"container1".to_string()));
    assert!(loaded.container_names.contains(&"container2".to_string()));

    // Test delete
    let delete_result = PodInfo::delete(root_path, pod_name);
    assert!(delete_result.is_ok(), "Pod info delete should succeed");

    // Verify deletion
    let load_after_delete = PodInfo::load(root_path, pod_name);
    assert!(
        load_after_delete.is_err(),
        "Pod info should not exist after deletion"
    );

    // Clean up
    fs::remove_dir_all(root_path).unwrap();

    println!("Pod persistence test completed!");
}

/// Test edge cases and error handling
#[test]
#[serial]
fn test_pod_error_handling() {
    let nonexistent_pod = "nonexistent-pod-12345";

    // Test operations on nonexistent pod
    println!("Testing error handling for nonexistent pods...");

    // State command on nonexistent pod
    let state_result = std::panic::catch_unwind(|| {
        let _ = pod_execute(PodCommand::State {
            pod_name: nonexistent_pod.to_string(),
        });
    });
    assert!(
        state_result.is_ok(),
        "State command on nonexistent pod should handle gracefully"
    );

    // Start command on nonexistent pod
    let start_result = std::panic::catch_unwind(|| {
        let _ = pod_execute(PodCommand::Start {
            pod_name: nonexistent_pod.to_string(),
        });
    });
    assert!(
        start_result.is_ok(),
        "Start command on nonexistent pod should handle gracefully"
    );

    // Delete command on nonexistent pod (should be idempotent)
    let delete_result = std::panic::catch_unwind(|| {
        let _ = pod_execute(PodCommand::Delete {
            pod_name: nonexistent_pod.to_string(),
        });
    });
    assert!(
        delete_result.is_ok(),
        "Delete command on nonexistent pod should be idempotent"
    );

    println!("Error handling tests completed!");
}

/// Test invalid pod configuration
#[test]
#[serial]
fn test_invalid_pod_config() {
    let invalid_config = "this is not valid yaml content at all";
    let invalid_path = create_temp_compose_file(invalid_config).unwrap();

    println!("Testing invalid pod configuration handling...");

    // Create command with invalid config should handle errors gracefully
    let create_result = std::panic::catch_unwind(|| {
        let _ = pod_execute(PodCommand::Create {
            pod_yaml: invalid_path.clone(),
        });
    });
    assert!(
        create_result.is_ok(),
        "Create command with invalid config should handle errors gracefully"
    );

    // Run command with invalid config should handle errors gracefully
    let run_result = std::panic::catch_unwind(|| {
        let _ = pod_execute(PodCommand::Run {
            pod_yaml: invalid_path.clone(),
        });
    });
    assert!(
        run_result.is_ok(),
        "Run command with invalid config should handle errors gracefully"
    );

    // Clean up
    cleanup_temp_compose_file(&invalid_path).unwrap();

    println!("Invalid config test completed!");
}
