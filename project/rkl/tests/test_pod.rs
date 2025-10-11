use crate::test_common::get_pod_config;
use anyhow::anyhow;
//use rkl::task::PodTask;
use common::{ContainerRes, PodTask, Resource};
use rkl::commands::pod::standalone::create_pod;
use rkl::commands::pod::standalone::delete_pod;
use rkl::commands::pod::standalone::start_pod;
use rkl::{commands::pod::run_pod, task::TaskRunner};
use serde_json::Value;
use serial_test::serial;
use std::{env, fs::File, io::Write, path::Path};
use tracing::error;

mod test_common;

#[test]
#[serial]
fn test_from_file() {
    let config = get_pod_config(vec!["echo".to_string()], "simple-container-task");
    let file_content = serde_yaml::to_string(&config).unwrap();
    let config_path = env::current_dir().unwrap().join("rkl/tests/test.yaml");
    let mut file = File::create(&config_path).unwrap();
    file.write_all(file_content.as_bytes()).unwrap();
    let runner = TaskRunner::from_file(config_path.as_os_str().to_str().unwrap()).unwrap();
    assert_eq!(
        runner.task.spec.containers[0].name,
        "simple-container-task-main-container1"
    );
    std::fs::remove_file(config_path).unwrap();
}

fn create(config: PodTask, run: bool) -> Result<(), anyhow::Error> {
    let file_content = serde_yaml::to_string(&config)?;
    let root = env::current_dir()?;
    let config_path = root.join("rkl/tests/test-pod.yaml");
    if Path::new(&config_path).exists() {
        std::fs::remove_file(&config_path)?;
    }
    let mut file = File::create(&config_path).map_err(|e| anyhow!("create failed: {e}"))?;
    file.write_all(file_content.as_bytes())?;

    // remove the container if for some reason, tests terminated before clean them.
    let pods_file = Path::new("/run/youki/pods/simple-container-task");
    let pause_dir = Path::new("/run/youki/simple-container-task");
    let container_dir = Path::new("/run/youki/simple-container-task-main-container1");
    if pods_file.exists() {
        std::fs::remove_file(pods_file)?;
    }
    if pause_dir.exists() {
        std::fs::remove_dir_all(pause_dir)?;
    }
    if container_dir.exists() {
        std::fs::remove_dir_all(container_dir)?;
    }

    if !run {
        create_pod(config_path.to_str().unwrap())?;
    } else {
        run_pod(config_path.to_str().unwrap())?;
    }
    std::fs::remove_file(config_path)?;
    Ok(())
}

fn try_create(config: PodTask, run: bool) {
    let res = create(config, run);
    if res.is_err() {
        error!(
            "\
            Failed to create pod. This may be not a test failed, but caused by wrong config.\n\
            tips:\n\
                1. Please build libipam and libbridge before running test.\n\
                2. You need run tests under root.\n\
        "
        );
        panic!("{}", res.unwrap_err());
    }
}

fn delete(pod_name: &str) {
    delete_pod(pod_name).unwrap();
}

#[test]
#[serial]
fn test_create_start_and_delete() {
    let config = get_pod_config(
        vec!["sleep".to_string(), "100".to_string()],
        "simple-container-task",
    );
    try_create(config, false);

    let container_state =
        std::fs::read_to_string("/run/youki/simple-container-task-main-container1/state.json")
            .unwrap();
    let pause_state =
        std::fs::read_to_string("/run/youki/simple-container-task/state.json").unwrap();
    let container_state: Value = serde_json::from_str(&container_state).unwrap();
    let pause_state: Value = serde_json::from_str(&pause_state).unwrap();
    assert_eq!(container_state["status"], "created");
    assert_eq!(pause_state["status"], "running");

    start_pod("simple-container-task").unwrap();

    let container_state =
        std::fs::read_to_string("/run/youki/simple-container-task-main-container1/state.json")
            .unwrap();
    let container_state: Value = serde_json::from_str(&container_state).unwrap();
    assert_eq!(container_state["status"], "running");

    delete_pod("simple-container-task").unwrap();

    assert!(!Path::new("/run/youki/simple-container-task-main-container1").exists());
    assert!(!Path::new("/run/youki/simple-container-task").exists());
    assert!(!Path::new("/run/youki/pods/simple-container-task").exists());
}

#[test]
#[serial]
fn test_run_pod() {
    let config = get_pod_config(
        vec!["sleep".to_string(), "100".to_string()],
        "simple-container-task",
    );
    try_create(config, true);
    let container_state =
        std::fs::read_to_string("/run/youki/simple-container-task-main-container1/state.json")
            .unwrap();
    let pause_state =
        std::fs::read_to_string("/run/youki/simple-container-task/state.json").unwrap();
    let container_state: Value = serde_json::from_str(&container_state).unwrap();
    let pause_state: Value = serde_json::from_str(&pause_state).unwrap();
    assert_eq!(container_state["status"], "running");
    assert_eq!(pause_state["status"], "running");

    delete("simple-container-task");
}

#[test]
#[serial]
fn test_cpu_and_memory_limit() {
    let mut config = get_pod_config(
        vec!["sleep".to_string(), "100".to_string()],
        "simple-container-task",
    );
    config.spec.containers[0].resources = Some(ContainerRes {
        limits: Some(Resource {
            cpu: Some("500m".to_string()),
            memory: Some("233Mi".to_string()),
        }),
    });
    try_create(config, true);

    let cpu_max = std::fs::read_to_string(
        "/sys/fs/cgroup/:youki:simple-container-task-main-container1/cpu.max",
    )
    .unwrap();
    assert_eq!(cpu_max.trim(), "500000 1000000");

    let memory_max = std::fs::read_to_string(
        "/sys/fs/cgroup/:youki:simple-container-task-main-container1/memory.max",
    )
    .unwrap();
    assert_eq!(memory_max.trim(), format!("{}", 233 * 1024 * 1024));

    delete("simple-container-task");
}
