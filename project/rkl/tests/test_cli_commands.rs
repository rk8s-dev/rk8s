use std::{collections::HashMap, env, fs::File, io::Write, path::Path};

use anyhow::anyhow;
use rkl::{
    cli_commands,
    task::{ContainerRes, ContainerSpec, ObjectMeta, PodSpec, PodTask, Port, Resource, TaskRunner},
};
use serde_json::Value;
use serial_test::serial;

fn bundles_path(name: &str) -> String {
    let root_dir = env::current_dir().unwrap();
    root_dir
        .join("test/bundles/")
        .join(name)
        .to_str()
        .unwrap()
        .to_string()
}

fn get_pod_config(args: Vec<String>) -> PodTask {
    PodTask {
        api_version: "v1".to_string(),
        kind: "Pod".to_string(),
        metadata: ObjectMeta {
            name: "simple-container-task".to_string(),
            labels: HashMap::from([
                ("app".to_string(), "my-app".to_string()),
                ("bundle".to_string(), bundles_path("pause")),
            ]),
            ..Default::default()
        },
        spec: PodSpec {
            containers: vec![ContainerSpec {
                name: "main-container1".to_string(),
                image: bundles_path("busybox"),
                args,
                ports: vec![Port {
                    container_port: 80,
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        },
    }
}

#[test]
#[serial]
fn test_from_file() {
    let config = get_pod_config(vec!["echo".to_string()]);
    let file_content = serde_yaml::to_string(&config).unwrap();
    let mut file = File::create("test/test.yaml").unwrap();
    file.write_all(file_content.as_bytes()).unwrap();
    let runner = TaskRunner::from_file("test/test.yaml").unwrap();
    assert_eq!(
        runner.task.spec.containers[0].name,
        "simple-container-task-main-container1"
    );
    std::fs::remove_file("test/test.yaml").unwrap();
}

fn create(config: PodTask, run: bool) -> Result<(), anyhow::Error> {
    let file_content = serde_yaml::to_string(&config)?;

    if Path::new("test/test-pod.yaml").exists() {
        std::fs::remove_file("test/test-pod.yaml")?;
    }
    let mut file = File::create("test/test-pod.yaml").map_err(|_| anyhow!("create failed"))?;
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

    let root = env::current_dir()?;
    env::set_current_dir(root.parent().unwrap().join("target/debug"))
        .map_err(|_| anyhow!("Failed to set current dir"))?;
    let config_path = root.join("test/test-pod.yaml");
    if !run {
        cli_commands::create_pod(config_path.to_str().unwrap())?;
    } else {
        cli_commands::run_pod(config_path.to_str().unwrap())?;
    }
    env::set_current_dir(root).map_err(|_| anyhow!("Failed to set back current dir"))?;
    std::fs::remove_file("test/test-pod.yaml")?;
    Ok(())
}

fn try_create(config: PodTask, run: bool) {
    let res = create(config, run);
    if res.is_err() {
        println!(
            "\
            Failed to create pod. This may be not a test failed, but caused by wrong config.\
            tips:\n\
                1. You should create test netns before test. Try to run `sudo ip netns add test`.\n\
                2. Run `sudo cargo test` instead of `cargo test`.\n\
                3. Please build libipam and libbridge before running test.\n\
        "
        );
        panic!("{}", res.unwrap_err());
    }
}

fn delete(pod_name: &str) {
    let root = env::current_dir().unwrap();
    env::set_current_dir(root.parent().unwrap().join("target/debug"))
        .map_err(|_| anyhow!("Failed to set current dir"))
        .unwrap();
    cli_commands::delete_pod(pod_name).unwrap();
    env::set_current_dir(root)
        .map_err(|_| anyhow!("Failed to set back current dir"))
        .unwrap();
}

#[test]
#[serial]
fn test_create_start_and_delete() {
    let config = get_pod_config(vec!["sleep".to_string(), "100".to_string()]);
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

    let root = env::current_dir().unwrap();
    env::set_current_dir(root.parent().unwrap().join("target/debug"))
        .map_err(|_| anyhow!("Failed to set current dir"))
        .unwrap();
    cli_commands::start_pod("simple-container-task").unwrap();

    let container_state =
        std::fs::read_to_string("/run/youki/simple-container-task-main-container1/state.json")
            .unwrap();
    let container_state: Value = serde_json::from_str(&container_state).unwrap();
    assert_eq!(container_state["status"], "running");

    cli_commands::delete_pod("simple-container-task").unwrap();
    env::set_current_dir(root)
        .map_err(|_| anyhow!("Failed to set back current dir"))
        .unwrap();

    assert!(!Path::new("/run/youki/simple-container-task-main-container1").exists());
    assert!(!Path::new("/run/youki/simple-container-task").exists());
    assert!(!Path::new("/run/youki/pods/simple-container-task").exists());
}

#[test]
#[serial]
fn test_run_pod() {
    let config = get_pod_config(vec!["sleep".to_string(), "100".to_string()]);
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
    let mut config = get_pod_config(vec!["sleep".to_string(), "100".to_string()]);
    config.spec.containers[0].resources = Some(ContainerRes{
        limits: Some(Resource {
            cpu: Some("500m".to_string()),
            memory: Some("233Mi".to_string()),
        })
    });
    try_create(config, true);
    
    let cpu_max = std::fs::read_to_string("/sys/fs/cgroup/:youki:simple-container-task-main-container1/cpu.max").unwrap();
    assert_eq!(cpu_max.trim(), "500000 1000000");

    let memory_max = std::fs::read_to_string("/sys/fs/cgroup/:youki:simple-container-task-main-container1/memory.max").unwrap();
    assert_eq!(memory_max.trim(), format!("{}", 233 * 1024 * 1024));

    delete("simple-container-task");
}