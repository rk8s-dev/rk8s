// Remove unused imports to resolve unused import warnings
// use anyhow::Result;
use serial_test::serial;
use std::fs;
use std::path::Path;

use rkb::commands::pod::{PodCommand, PodInfo, pod_execute};
use rkb::pod_task::TaskRunner;
use test_common::*;

mod test_common;

/// Get pod configuration for testing
fn get_pod_config(pod_name: &str) -> String {
    format!(
        r#"apiVersion: v1
kind: Pod
metadata:
  name: {}
spec:
  containers:
  - name: test-container
    image: {}  
    command: ["sleep", "10"]
    ports:
    - containerPort: 8080
  restartPolicy: Never
"#,
        pod_name,
        bundles_path("busybox")
    )
}

/// Test basic functionality of TaskRunner
#[test]
#[serial]
fn test_task_runner() {
    let pod_name = "test-task-runner-pod";
    let pod_config = get_pod_config(pod_name);

    // Create temporary pod configuration file
    let pod_path = create_temp_compose_file(&pod_config).unwrap();

    // Ensure previous test resources are cleaned up
    cleanup_pod(pod_name).unwrap();

    // Test TaskRunner::from_file (strict assertion: must succeed, remove {:?} formatting)
    let result = TaskRunner::from_file(&pod_path);
    assert!(
        result.is_ok(),
        "TaskRunner::from_file failed, Pod configuration loading logic has porting issues"
    );

    // Remove invalid pod_name field assertion (TaskRunner doesn't have this field)
    // let task_runner = result.unwrap();
    // assert_eq!(task_runner.pod_name, pod_name, "Pod name in TaskRunner doesn't match configuration");

    // Clean up temporary files
    cleanup_temp_compose_file(&pod_path).unwrap();
    cleanup_pod(pod_name).unwrap();
}

/// Test complete Pod lifecycle: create -> state -> delete
#[test]
#[serial]
fn test_pod_lifecycle() {
    let pod_name = "test-pod-lifecycle";
    let pod_config = get_pod_config(pod_name);

    // Create temporary pod configuration file
    let pod_path = create_temp_compose_file(&pod_config).unwrap();

    // Ensure previous test resources are cleaned up
    cleanup_pod(pod_name).unwrap();

    // Test pod create command (pod_execute returns (), only verify execution doesn't panic)
    // Use std::panic::catch_unwind to catch panics, replace Result assertion
    let create_result = std::panic::catch_unwind(|| {
        let _ = pod_execute(PodCommand::Create {
            pod_yaml: pod_path.clone(),
        });
    });
    assert!(
        create_result.is_ok(),
        "Pod create command panicked, porting issue"
    );

    // Test pod state command (verify execution doesn't panic)
    let state_result = std::panic::catch_unwind(|| {
        let _ = pod_execute(PodCommand::State {
            pod_name: pod_name.to_string(),
        });
    });
    assert!(
        state_result.is_ok(),
        "Pod state command panicked, porting issue"
    );

    // Remove invalid contains assertion (pod_execute returns (), no string return value)
    // let state_output = state_result.unwrap();
    // assert!(state_output.contains(pod_name), "Pod state result doesn't contain Pod name");

    // Test pod delete command (verify execution doesn't panic)
    let delete_result = std::panic::catch_unwind(|| {
        let _ = pod_execute(PodCommand::Delete {
            pod_name: pod_name.to_string(),
        });
    });
    assert!(
        delete_result.is_ok(),
        "Pod delete command panicked, porting issue"
    );

    // Clean up temporary files
    cleanup_temp_compose_file(&pod_path).unwrap();
    cleanup_pod(pod_name).unwrap();
}

/// Test PodInfo structure functionality
#[test]
#[serial]
fn test_pod_info() {
    let pod_name = "test-pod-info";
    let root_path = Path::new("/tmp"); // Use /tmp directory to avoid permission issues

    // Create PodInfo
    let pod_info = PodInfo {
        pod_sandbox_id: "test-sandbox-id".to_string(),
        container_names: vec![
            "test-container-1".to_string(),
            "test-container-2".to_string(),
        ],
    };

    // Test PodInfo::save (use /tmp directory to avoid permission issues)
    let save_result = pod_info.save(root_path, pod_name);
    assert!(save_result.is_ok(), "PodInfo::save failed, porting issue");

    // Test PodInfo::delete
    let delete_result = PodInfo::delete(root_path, pod_name);
    assert!(
        delete_result.is_ok(),
        "PodInfo::delete failed, porting issue"
    );

    // Clean up temporary files
    let pod_info_path = root_path.join("pods").join(pod_name);
    if pod_info_path.exists() {
        fs::remove_file(pod_info_path).unwrap();
    }
    let pods_dir = root_path.join("pods");
    if pods_dir.exists() {
        fs::remove_dir(pods_dir).unwrap();
    }
}

/// Test parsing of all Pod commands
#[test]
#[serial]
fn test_all_pod_commands() {
    let pod_name = "test-all-commands-pod";
    let pod_config = get_pod_config(pod_name);

    // Create temporary pod configuration file
    let pod_path = create_temp_compose_file(&pod_config).unwrap();

    // Ensure previous test resources are cleaned up
    cleanup_pod(pod_name).unwrap();

    // Test pod run command (verify execution doesn't panic)
    let run_result = std::panic::catch_unwind(|| {
        let _ = pod_execute(PodCommand::Run {
            pod_yaml: pod_path.clone(),
        });
    });
    assert!(
        run_result.is_ok(),
        "Pod run command panicked, porting issue"
    );

    // Test pod create command (verify execution doesn't panic)
    let create_result = std::panic::catch_unwind(|| {
        let _ = pod_execute(PodCommand::Create {
            pod_yaml: pod_path.clone(),
        });
    });
    assert!(
        create_result.is_ok(),
        "Pod create command panicked, porting issue"
    );

    // Test pod start command (verify execution doesn't panic)
    let start_result = std::panic::catch_unwind(|| {
        let _ = pod_execute(PodCommand::Start {
            pod_name: pod_name.to_string(),
        });
    });
    assert!(
        start_result.is_ok(),
        "Pod start command panicked, porting issue"
    );

    // Test pod list command (verify execution doesn't panic)
    let list_result = std::panic::catch_unwind(|| {
        let _ = pod_execute(PodCommand::List {});
    });
    assert!(
        list_result.is_ok(),
        "Pod list command panicked, porting issue"
    );

    // Remove invalid is_empty assertion (pod_execute returns (), no string return value)
    // let list_output = list_result.unwrap();
    // assert!(!list_output.is_empty(), "Pod list returns empty, porting issue");

    // Test pod state command (verify execution doesn't panic)
    let state_result = std::panic::catch_unwind(|| {
        let _ = pod_execute(PodCommand::State {
            pod_name: pod_name.to_string(),
        });
    });
    assert!(
        state_result.is_ok(),
        "Pod state command panicked, porting issue"
    );

    // Test pod delete command (verify execution doesn't panic)
    let delete_result = std::panic::catch_unwind(|| {
        let _ = pod_execute(PodCommand::Delete {
            pod_name: pod_name.to_string(),
        });
    });
    assert!(
        delete_result.is_ok(),
        "Pod delete command panicked, porting issue"
    );

    // Clean up temporary files
    cleanup_temp_compose_file(&pod_path).unwrap();
    cleanup_pod(pod_name).unwrap();
}

/// Test invalid Pod configuration
#[test]
#[serial]
fn test_invalid_pod_config() {
    // Create invalid pod configuration
    let invalid_config = "invalid yaml content";
    let pod_path = create_temp_compose_file(invalid_config).unwrap();

    // Test pod create command (invalid configuration should return error instead of panic)
    let result = std::panic::catch_unwind(|| {
        let _ = pod_execute(PodCommand::Create {
            pod_yaml: pod_path.clone(),
        });
    });
    // Invalid YAML configuration should be properly handled, return error instead of panic
    assert!(
        result.is_ok(),
        "Pod create with invalid configuration should not panic, but return error"
    );

    // Clean up temporary files
    cleanup_temp_compose_file(&pod_path).unwrap();
}

/// Test nonexistent Pod
#[test]
#[serial]
fn test_nonexistent_pod() {
    let nonexistent_pod = "nonexistent-pod-12345";

    // Ensure this Pod doesn't exist
    cleanup_pod(nonexistent_pod).unwrap();

    // Test pod state command (querying nonexistent Pod should return error instead of panic)
    let state_result = std::panic::catch_unwind(|| {
        let _ = pod_execute(PodCommand::State {
            pod_name: nonexistent_pod.to_string(),
        });
    });
    assert!(
        state_result.is_ok(),
        "Querying state of nonexistent Pod should not panic, but return error"
    );

    // Test pod delete command (idempotency: execution doesn't panic)
    let delete_result = std::panic::catch_unwind(|| {
        let _ = pod_execute(PodCommand::Delete {
            pod_name: nonexistent_pod.to_string(),
        });
    });
    assert!(
        delete_result.is_ok(),
        "Deleting nonexistent Pod should not panic (idempotent), porting delete logic has issues"
    );
}

/// Quick test for all pod commands - verify they don't panic with basic input
#[test]
#[serial]
fn test_all_pod_commands_quick() {
    let pod_name = "quick-test-pod";
    let pod_config = get_pod_config(pod_name);

    // Create temporary pod configuration file
    let pod_path = create_temp_compose_file(&pod_config).unwrap();

    // Ensure clean state
    cleanup_pod(pod_name).unwrap();

    // Test all commands in sequence
    println!("Quick testing all pod commands...");

    // Test create
    let create_result = std::panic::catch_unwind(|| {
        let _ = pod_execute(PodCommand::Create {
            pod_yaml: pod_path.clone(),
        });
    });
    assert!(create_result.is_ok(), "Pod create should not panic");

    // Test list
    let list_result = std::panic::catch_unwind(|| {
        let _ = pod_execute(PodCommand::List {});
    });
    assert!(list_result.is_ok(), "Pod list should not panic");

    // Test state
    let state_result = std::panic::catch_unwind(|| {
        let _ = pod_execute(PodCommand::State {
            pod_name: pod_name.to_string(),
        });
    });
    assert!(state_result.is_ok(), "Pod state should not panic");

    // Test start
    let start_result = std::panic::catch_unwind(|| {
        let _ = pod_execute(PodCommand::Start {
            pod_name: pod_name.to_string(),
        });
    });
    assert!(start_result.is_ok(), "Pod start should not panic");

    // Test delete
    let delete_result = std::panic::catch_unwind(|| {
        let _ = pod_execute(PodCommand::Delete {
            pod_name: pod_name.to_string(),
        });
    });
    assert!(delete_result.is_ok(), "Pod delete should not panic");

    // Test run with a different pod
    let run_pod_name = "quick-run-test-pod";
    let run_pod_config = get_pod_config(run_pod_name);
    let run_pod_path = create_temp_compose_file(&run_pod_config).unwrap();

    cleanup_pod(run_pod_name).unwrap();

    let run_result = std::panic::catch_unwind(|| {
        let _ = pod_execute(PodCommand::Run {
            pod_yaml: run_pod_path.clone(),
        });
    });
    assert!(run_result.is_ok(), "Pod run should not panic");

    // Clean up
    let _ = std::panic::catch_unwind(|| {
        let _ = pod_execute(PodCommand::Delete {
            pod_name: run_pod_name.to_string(),
        });
    });

    cleanup_temp_compose_file(&pod_path).unwrap();
    cleanup_temp_compose_file(&run_pod_path).unwrap();
    cleanup_pod(pod_name).unwrap();
    cleanup_pod(run_pod_name).unwrap();

    println!("Quick test of all pod commands completed successfully!");
}
