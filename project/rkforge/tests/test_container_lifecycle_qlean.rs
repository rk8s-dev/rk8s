//! Container lifecycle integration tests using qlean VM isolation.
//!
//! These tests run inside a QEMU/KVM virtual machine using the qlean crate,
//! ensuring complete isolation and avoiding local environment pollution.

use std::path::{Path, PathBuf};
use std::sync::Once;
use std::time::Duration;

use anyhow::{Context, Result};
use qlean::{Distro, Machine, MachineConfig, create_image, with_machine};
use tracing_subscriber::EnvFilter;

const RKFORGE_BIN_IN_VM: &str = "/usr/local/bin/rkforge";
const YOUKI_STATE_DIR: &str = "/run/youki";
const TEST_BUNDLE_DIR: &str = "/tmp/test-bundles/busybox";

fn tracing_subscriber_init() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .init();
    });
}

async fn exec_check(vm: &mut Machine, cmd: &str) -> Result<String> {
    let result = vm.exec(cmd).await?;
    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        let stdout = String::from_utf8_lossy(&result.stdout);
        anyhow::bail!(
            "Command '{}' failed with exit code {:?}\nstdout: {}\nstderr: {}",
            cmd,
            result.status.code(),
            stdout,
            stderr
        );
    }
    Ok(String::from_utf8_lossy(&result.stdout).to_string())
}

fn get_rkforge_binary_path() -> Result<PathBuf> {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_rkforge") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Ok(path);
        }
    }

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").context("CARGO_MANIFEST_DIR not set")?;
    let target_dir = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(&manifest_dir)
                .parent()
                .unwrap()
                .join("target")
        });

    for profile in &["debug", "release"] {
        let path = target_dir.join(format!("{}/rkforge", profile));
        if path.exists() {
            return Ok(path);
        }
    }

    anyhow::bail!("rkforge binary not found. Build it first with 'cargo build -p rkforge'.");
}

fn get_test_bundle_path() -> Result<PathBuf> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").context("CARGO_MANIFEST_DIR not set")?;
    let bundle_path = PathBuf::from(&manifest_dir)
        .parent()
        .unwrap()
        .join("test/bundles/busybox");

    if !bundle_path.exists() {
        anyhow::bail!(
            "Test bundle not found at {:?}. Please ensure test bundles are available.",
            bundle_path
        );
    }

    Ok(bundle_path)
}

async fn setup_vm_environment(vm: &mut Machine) -> Result<()> {
    tracing::info!("Installing dependencies...");
    exec_check(vm, "apt-get update -qq").await?;
    exec_check(
        vm,
        "DEBIAN_FRONTEND=noninteractive apt-get install -y -qq curl jq coreutils util-linux procps",
    )
    .await?;

    exec_check(vm, &format!("mkdir -p {}", YOUKI_STATE_DIR)).await?;
    exec_check(vm, &format!("mkdir -p {}", TEST_BUNDLE_DIR)).await?;

    tracing::info!("VM environment setup complete.");
    Ok(())
}

async fn upload_youki_binary(vm: &mut Machine) -> Result<()> {
    // Try to find youki binary on the host
    let youki_paths = vec![
        "/home/ckd/.cargo/bin/youki",
        "/usr/local/bin/youki",
        "/usr/bin/youki",
    ];

    let mut youki_path = None;
    for path in youki_paths {
        if std::path::Path::new(path).exists() {
            youki_path = Some(path);
            break;
        }
    }

    if let Some(path) = youki_path {
        tracing::info!("Uploading youki binary from {}...", path);
        vm.upload(
            std::path::Path::new(path),
            std::path::Path::new("/usr/local/bin"),
        )
        .await?;
        exec_check(vm, "chmod +x /usr/local/bin/youki").await?;
        tracing::info!("youki binary uploaded successfully.");
    } else {
        tracing::warn!("youki binary not found on host, skipping upload");
    }

    Ok(())
}

async fn upload_test_artifacts(
    vm: &mut Machine,
    rkforge_bin: &Path,
    bundle_path: &Path,
) -> Result<()> {
    tracing::info!("Uploading rkforge binary...");
    vm.upload(rkforge_bin, Path::new("/usr/local/bin")).await?;
    exec_check(vm, &format!("chmod +x {}", RKFORGE_BIN_IN_VM)).await?;

    tracing::info!("Uploading test bundle...");
    let bundle_parent = Path::new(TEST_BUNDLE_DIR).parent().unwrap();
    exec_check(vm, &format!("mkdir -p {}", bundle_parent.display())).await?;

    let temp_tar = std::env::temp_dir().join("busybox-bundle.tar.gz");
    let tar_result = std::process::Command::new("tar")
        .args(&[
            "-czf",
            temp_tar.to_str().unwrap(),
            "-C",
            bundle_path.parent().unwrap().to_str().unwrap(),
            bundle_path.file_name().unwrap().to_str().unwrap(),
        ])
        .output()
        .context("Failed to create bundle tarball")?;

    if !tar_result.status.success() {
        anyhow::bail!("Failed to tar bundle: {:?}", tar_result);
    }

    vm.upload(&temp_tar, Path::new("/tmp")).await?;
    exec_check(
        vm,
        &format!(
            "tar -xzf /tmp/busybox-bundle.tar.gz -C {}",
            bundle_parent.display()
        ),
    )
    .await?;
    std::fs::remove_file(&temp_tar).ok();

    tracing::info!("Test artifacts uploaded successfully.");
    Ok(())
}

fn unique_container_name(prefix: &str) -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("{}-{}-{}", prefix, ts, std::process::id())
}

async fn create_container_config(
    vm: &mut Machine,
    container_name: &str,
    command: &[&str],
) -> Result<String> {
    let config_path = format!("/tmp/{}.yaml", container_name);

    // Build args as YAML array
    let args_yaml = command
        .iter()
        .map(|item| format!("  - \"{}\"", item))
        .collect::<Vec<_>>()
        .join("\n");

    let content = format!(
        "name: {}\nimage: {}\nargs:\n{}\n",
        container_name, TEST_BUNDLE_DIR, args_yaml
    );

    vm.write(Path::new(&config_path), content.as_bytes())
        .await?;
    Ok(config_path)
}

async fn wait_for_container_status(
    vm: &mut Machine,
    container_name: &str,
    expected_status: &str,
    timeout_secs: u64,
) -> Result<()> {
    let start = std::time::Instant::now();
    let timeout = Duration::from_secs(timeout_secs);

    loop {
        let result = vm
            .exec(&format!(
                "{} state {} 2>&1",
                RKFORGE_BIN_IN_VM, container_name
            ))
            .await?;

        let output = String::from_utf8_lossy(&result.stdout);
        let stderr = String::from_utf8_lossy(&result.stderr);

        // Parse the status from the table output
        // Format: ID\tPID\tSTATUS\tBUNDLE\tCREATED\tCREATOR
        let status = if result.status.success() {
            output
                .lines()
                .skip(1) // Skip header
                .next()
                .and_then(|line| {
                    line.split_whitespace().nth(2) // STATUS is 3rd column
                })
                .map(|s| s.to_lowercase())
        } else {
            None
        };

        if let Some(status) = status {
            if status.contains(expected_status) {
                tracing::info!(
                    "Container {} reached status: {}",
                    container_name,
                    expected_status
                );
                return Ok(());
            }
        } else if expected_status == "not_found" && !result.status.success() {
            // Container doesn't exist, which is expected for cleanup verification
            return Ok(());
        }

        if start.elapsed() > timeout {
            anyhow::bail!(
                "Timeout waiting for container {} to reach status '{}'. Last output: {}\nstderr: {}",
                container_name,
                expected_status,
                output,
                stderr
            );
        }

        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

async fn verify_container_cleanup(vm: &mut Machine, container_name: &str) -> Result<()> {
    let state_dir = format!("{}/{}", YOUKI_STATE_DIR, container_name);
    let result = vm
        .exec(&format!(
            "test -d {} && echo exists || echo clean",
            state_dir
        ))
        .await?;
    let output = String::from_utf8_lossy(&result.stdout);

    if output.trim() != "clean" {
        anyhow::bail!("Container state directory still exists: {}", state_dir);
    }

    let cgroup_path = format!("/sys/fs/cgroup/:youki:{}", container_name);
    let result = vm
        .exec(&format!(
            "test -d {} && echo exists || echo clean",
            cgroup_path
        ))
        .await?;
    let output = String::from_utf8_lossy(&result.stdout);

    if output.trim() != "clean" {
        anyhow::bail!("Container cgroup still exists: {}", cgroup_path);
    }

    tracing::info!("Container {} cleanup verified", container_name);
    Ok(())
}

async fn test_create_start_stop_rm(vm: &mut Machine) -> Result<()> {
    tracing::info!("--- Test Case 1: create -> start -> stop -> rm ---");

    let container_name = unique_container_name("test-create");
    let config_path = create_container_config(vm, &container_name, &["sleep", "300"]).await?;

    tracing::info!("Creating container {}...", container_name);
    exec_check(vm, &format!("{} create {}", RKFORGE_BIN_IN_VM, config_path)).await?;
    wait_for_container_status(vm, &container_name, "created", 10).await?;

    tracing::info!("Starting container {}...", container_name);
    exec_check(
        vm,
        &format!("{} start {}", RKFORGE_BIN_IN_VM, container_name),
    )
    .await?;
    wait_for_container_status(vm, &container_name, "running", 10).await?;

    tracing::info!("Stopping container {} with SIGTERM...", container_name);
    // Use youki kill command directly since rkforge doesn't expose kill in CLI
    // Need to specify --root to point to youki state directory
    exec_check(
        vm,
        &format!(
            "youki --root {} kill {} SIGTERM",
            YOUKI_STATE_DIR, container_name
        ),
    )
    .await?;
    wait_for_container_status(vm, &container_name, "stopped", 10).await?;

    tracing::info!("Removing container {}...", container_name);
    exec_check(
        vm,
        &format!("{} delete {}", RKFORGE_BIN_IN_VM, container_name),
    )
    .await?;

    verify_container_cleanup(vm, &container_name).await?;

    tracing::info!("[SUCCESS] Test Case 1 passed");
    Ok(())
}

async fn test_run_kill_rm(vm: &mut Machine) -> Result<()> {
    tracing::info!("--- Test Case 2: run -> kill -> rm ---");

    let container_name = unique_container_name("test-run");
    let config_path = create_container_config(vm, &container_name, &["sleep", "300"]).await?;

    tracing::info!("Running container {}...", container_name);
    // Run in background since run command blocks
    exec_check(
        vm,
        &format!(
            "nohup {} run {} > /tmp/{}.log 2>&1 &",
            RKFORGE_BIN_IN_VM, config_path, container_name
        ),
    )
    .await?;

    // Give it time to start
    tokio::time::sleep(Duration::from_secs(2)).await;
    wait_for_container_status(vm, &container_name, "running", 10).await?;

    tracing::info!("Killing container {} with SIGKILL...", container_name);
    // Use youki kill command directly with --root option
    exec_check(
        vm,
        &format!(
            "youki --root {} kill {} SIGKILL",
            YOUKI_STATE_DIR, container_name
        ),
    )
    .await?;
    wait_for_container_status(vm, &container_name, "stopped", 10).await?;

    tracing::info!("Removing container {}...", container_name);
    exec_check(
        vm,
        &format!("{} delete {}", RKFORGE_BIN_IN_VM, container_name),
    )
    .await?;

    verify_container_cleanup(vm, &container_name).await?;

    tracing::info!("[SUCCESS] Test Case 2 passed");
    Ok(())
}

async fn test_concurrent_containers(vm: &mut Machine) -> Result<()> {
    tracing::info!("--- Test Case 3: Concurrent containers ---");

    let mut containers = Vec::new();
    for i in 1..=3 {
        let container_name = unique_container_name(&format!("test-concurrent-{}", i));
        let config_path = create_container_config(vm, &container_name, &["sleep", "60"]).await?;

        tracing::info!("Running container {}...", container_name);
        // Run in background
        exec_check(
            vm,
            &format!(
                "nohup {} run {} > /tmp/{}.log 2>&1 &",
                RKFORGE_BIN_IN_VM, config_path, container_name
            ),
        )
        .await?;

        containers.push(container_name);
    }

    // Give containers time to start
    tokio::time::sleep(Duration::from_secs(2)).await;

    for container_name in &containers {
        wait_for_container_status(vm, container_name, "running", 10).await?;
    }

    for container_name in &containers {
        tracing::info!("Killing container {}...", container_name);
        exec_check(
            vm,
            &format!(
                "youki --root {} kill {} SIGKILL",
                YOUKI_STATE_DIR, container_name
            ),
        )
        .await?;
        wait_for_container_status(vm, container_name, "stopped", 10).await?;

        tracing::info!("Removing container {}...", container_name);
        exec_check(
            vm,
            &format!("{} delete {}", RKFORGE_BIN_IN_VM, container_name),
        )
        .await?;

        verify_container_cleanup(vm, container_name).await?;
    }

    tracing::info!("[SUCCESS] Test Case 3 passed");
    Ok(())
}

async fn run_tests_in_vm(vm: &mut Machine, rkforge_bin: &Path, bundle_path: &Path) -> Result<()> {
    setup_vm_environment(vm).await?;
    upload_youki_binary(vm).await?;
    upload_test_artifacts(vm, rkforge_bin, bundle_path).await?;

    test_create_start_stop_rm(vm).await?;
    test_run_kill_rm(vm).await?;
    test_concurrent_containers(vm).await?;

    tracing::info!("");
    tracing::info!("=================================================");
    tracing::info!("[SUCCESS] All rkforge lifecycle tests passed!");
    tracing::info!("=================================================");

    Ok(())
}

#[tokio::test]
#[ignore]
async fn test_rkforge_container_lifecycle() -> Result<()> {
    tracing_subscriber_init();

    let rkforge_bin = get_rkforge_binary_path()?;
    let bundle_path = get_test_bundle_path()?;

    tracing::info!("Using rkforge binary: {:?}", rkforge_bin);
    tracing::info!("Using test bundle: {:?}", bundle_path);

    tracing::info!("Creating VM image...");
    let image = create_image(Distro::Debian, "debian-13-generic-amd64").await?;
    let config = MachineConfig {
        core: 2,
        mem: 2048,
        disk: Some(10),
        clear: true,
    };

    let rkforge_bin = std::sync::Arc::new(rkforge_bin);
    let bundle_path = std::sync::Arc::new(bundle_path);

    with_machine(&image, &config, move |vm| {
        let rkforge_bin = std::sync::Arc::clone(&rkforge_bin);
        let bundle_path = std::sync::Arc::clone(&bundle_path);
        Box::pin(async move {
            tracing::info!("VM started successfully");
            run_tests_in_vm(vm, &rkforge_bin, &bundle_path).await
        })
    })
    .await?;

    Ok(())
}
