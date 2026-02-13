//! Integration test for rkforge Dockerfile build functionality.
//!
//! This test uses qlean (QEMU/KVM VM) to test the full build pipeline:
//! 1. Create a Debian VM
//! 2. Install necessary tools (podman, skopeo, etc.)
//! 3. Upload rkforge binary and test Dockerfile
//! 4. Run rkforge build
//! 5. Load the built image into podman
//! 6. Run and verify the container
//!
//! # Run Tests
//!
//! Run all tests:
//! ```bash
//! RUST_LOG=info cargo test --test test_build_dockerfile -- --nocapture 2>&1 | tee run.log
//! ```
//!
//! Run specific test:
//! ```bash
//! RUST_LOG=info cargo test --test test_build_dockerfile test_build_dockerfile_nginx -- --nocapture 2>&1 | tee run.log
//! RUST_LOG=info cargo test --test test_build_dockerfile test_build_dockerfile_memcached -- --nocapture 2>&1 | tee run.log
//! RUST_LOG=info cargo test --test test_build_dockerfile test_build_dockerfile_mysql -- --nocapture 2>&1 | tee run.log
//! RUST_LOG=info cargo test --test test_build_dockerfile test_build_dockerfile_redis -- --nocapture 2>&1 | tee run.log
//! RUST_LOG=info cargo test --test test_build_dockerfile test_build_dockerfile_postgres -- --nocapture 2>&1 | tee run.log
//! ```

use std::path::{Path, PathBuf};
use std::sync::Once;

use anyhow::{Context, Result};
use qlean::{Distro, Machine, MachineConfig, create_image, with_machine};
use tracing_subscriber::EnvFilter;

/// Initialize tracing subscriber (only once across all tests)
fn tracing_subscriber_init() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .init();
    });
}

/// Execute a command in the VM, print full stdout/stderr for visibility.
/// Returns stdout on success, or an error with details on failure.
async fn exec_show(vm: &mut Machine, step: &str, cmd: &str) -> Result<String> {
    println!("\n========================================");
    println!("➜ Step: [{}]", step);
    println!("  Command: {}", cmd);
    println!("========================================");

    let result = vm.exec(cmd).await?;
    let stdout = String::from_utf8_lossy(&result.stdout).to_string();
    let stderr = String::from_utf8_lossy(&result.stderr).to_string();

    // Always print output so build process is visible in the terminal
    if !stdout.trim().is_empty() {
        println!("{}", stdout);
    }
    if !stderr.trim().is_empty() {
        eprintln!("{}", stderr);
    }

    if !result.status.success() {
        anyhow::bail!(
            "Step [{}] failed with exit code {:?}\nSTDOUT:\n{}\nSTDERR:\n{}",
            step,
            result.status.code(),
            stdout,
            stderr
        );
    }

    Ok(stdout)
}

/// Get the path to the rkforge binary (debug or release build)
fn get_rkforge_binary_path() -> Result<PathBuf> {
    // Try the env var set by cargo for integration tests
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

    let debug_path = target_dir.join("debug/rkforge");
    if debug_path.exists() {
        return Ok(debug_path);
    }

    let release_path = target_dir.join("release/rkforge");
    if release_path.exists() {
        return Ok(release_path);
    }

    anyhow::bail!(
        "rkforge binary not found at {:?} or {:?}. Build it first with `cargo build -p rkforge`.",
        debug_path,
        release_path
    );
}

/// Get the path to the nginx-docker test directory
fn get_nginx_docker_dir() -> Result<PathBuf> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").context("CARGO_MANIFEST_DIR not set")?;
    let dir = PathBuf::from(&manifest_dir).join("tests/dockers/nginx-docker");
    if dir.exists() {
        return Ok(dir);
    }
    anyhow::bail!(
        "nginx-docker test directory not found at {:?}. Make sure the test resources exist.",
        dir
    );
}

/// Get the path to the memcached-docker test directory
fn get_memcached_docker_dir() -> Result<PathBuf> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").context("CARGO_MANIFEST_DIR not set")?;
    let dir = PathBuf::from(&manifest_dir).join("tests/dockers/memcached-docker");
    if dir.exists() {
        return Ok(dir);
    }
    anyhow::bail!(
        "memcached-docker test directory not found at {:?}. Make sure the test resources exist.",
        dir
    );
}

/// Get the path to the mysql-docker test directory
fn get_mysql_docker_dir() -> Result<PathBuf> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").context("CARGO_MANIFEST_DIR not set")?;
    let dir = PathBuf::from(&manifest_dir).join("tests/dockers/mysql-docker");
    if dir.exists() {
        return Ok(dir);
    }
    anyhow::bail!(
        "mysql-docker test directory not found at {:?}. Make sure the test resources exist.",
        dir
    );
}

/// Get the path to the redis-docker test directory
fn get_redis_docker_dir() -> Result<PathBuf> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").context("CARGO_MANIFEST_DIR not set")?;
    let dir = PathBuf::from(&manifest_dir).join("tests/dockers/redis-docker");
    if dir.exists() {
        return Ok(dir);
    }
    anyhow::bail!(
        "redis-docker test directory not found at {:?}. Make sure the test resources exist.",
        dir
    );
}

/// Get the path to the postgres-docker test directory
fn get_postgres_docker_dir() -> Result<PathBuf> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").context("CARGO_MANIFEST_DIR not set")?;
    let dir = PathBuf::from(&manifest_dir).join("tests/dockers/postgres-docker");
    if dir.exists() {
        return Ok(dir);
    }
    anyhow::bail!(
        "postgres-docker test directory not found at {:?}. Make sure the test resources exist.",
        dir
    );
}

/// Install required dependencies in the VM (podman, skopeo, curl, etc.)
async fn install_deps(vm: &mut Machine) -> Result<()> {
    exec_show(vm, "Update apt sources", "apt-get update -qq").await?;
    exec_show(
        vm,
        "Install podman, skopeo and tools",
        "DEBIAN_FRONTEND=noninteractive apt-get install -y -qq podman skopeo curl ca-certificates",
    )
    .await?;

    // Verify installations
    exec_show(vm, "Check podman version", "podman --version").await?;
    exec_show(vm, "Check skopeo version", "skopeo --version").await?;

    Ok(())
}

#[tokio::test]
async fn test_build_dockerfile_nginx() -> Result<()> {
    tracing_subscriber_init();

    // ===== Resolve local paths =====
    let rkforge_bin = get_rkforge_binary_path()?;
    let docker_dir = get_nginx_docker_dir()?;

    tracing::info!("Using rkforge binary at {:?}", rkforge_bin);
    tracing::info!("Using nginx-docker dir at {:?}", docker_dir);

    // ===== Create VM =====
    let image = create_image(Distro::Debian, "debian-13-generic-amd64").await?;
    let config = MachineConfig {
        core: 4,
        mem: 4096,
        disk: Some(20),
        clear: true,
    };

    with_machine(&image, &config, |vm| {
        Box::pin(async move {
            tracing::info!("VM started successfully");

            // ===== Step 1: Install dependencies =====
            install_deps(vm).await?;

            // ===== Step 2: Upload rkforge binary =====
            tracing::info!("Uploading rkforge binary...");
            vm.upload(&rkforge_bin, Path::new("/usr/local/bin")).await?;
            exec_show(
                vm,
                "Make rkforge executable",
                "chmod +x /usr/local/bin/rkforge",
            )
            .await?;
            exec_show(vm, "Verify rkforge binary", "file /usr/local/bin/rkforge").await?;

            // ===== Step 3: Upload test Dockerfile directory =====
            tracing::info!("Uploading nginx-docker test directory...");
            vm.create_dir_all(Path::new("/test")).await?;
            vm.upload(&docker_dir, Path::new("/test")).await?;

            // ===== Step 3.5: Make all .sh files executable =====
            exec_show(
                vm,
                "Make shell scripts executable",
                "find /test -name '*.sh' -exec chmod +x {} +",
            )
            .await?;

            // ===== Step 4: List uploaded files for verification =====
            exec_show(vm, "List test files", "ls -laR /test/").await?;

            // ===== Step 5: Run rkforge build =====
            exec_show(
                vm,
                "Build nginx image with rkforge",
                "cd /test/nginx-docker && sudo /usr/local/bin/rkforge build \
                    -f /test/nginx-docker/Dockerfile \
                    -t nginx-latest \
                    -o /test/output \
                    /test/nginx-docker",
            )
            .await?;

            // ===== Step 6: Verify build output =====
            exec_show(vm, "List build output", "ls -laR /test/output/").await?;

            // ===== Step 7: Load OCI layout into podman using skopeo =====
            exec_show(
                vm,
                "Load OCI layout into podman",
                "skopeo copy oci:/test/output/nginx-latest containers-storage:localhost/nginx-latest",
            )
            .await?;

            // ===== Step 8: List and verify images =====
            exec_show(vm, "List podman images", "podman images").await?;

            // ===== Step 9: Run nginx container =====
            exec_show(
                vm,
                "Run nginx container",
                "podman run -d --name nginx-test -p 8080:80 localhost/nginx-latest",
            )
            .await?;

            // Wait for container to start up
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;

            // ===== Step 10: Check container status =====
            exec_show(vm, "Check container status", "podman ps -a").await?;
            exec_show(
                vm,
                "Check container logs",
                "podman logs nginx-test 2>&1",
            )
            .await?;

            // ===== Step 11: Test nginx with curl =====
            let curl_output = exec_show(
                vm,
                "Test nginx with curl",
                "curl -sf http://localhost:8080 || curl -s http://localhost:8080",
            )
            .await?;

            // Verify the response contains nginx signature
            if curl_output.to_lowercase().contains("nginx")
                || curl_output.to_lowercase().contains("welcome")
            {
                println!("\n========================================");
                println!("[SUCCESS] Nginx container is running correctly!");
                println!("========================================");
                tracing::info!("[SUCCESS] Nginx container is running correctly!");
            } else {
                anyhow::bail!(
                    "Unexpected response from nginx container:\n{}",
                    curl_output
                );
            }

            // ===== Step 12: Cleanup =====
            exec_show(vm, "Stop container", "podman stop nginx-test || true").await?;
            exec_show(vm, "Remove container", "podman rm nginx-test || true").await?;
            exec_show(vm, "Remove image", "podman rmi localhost/nginx-latest || true").await?;
            exec_show(vm, "Clean up test files", "rm -rf /test || true").await?;

            tracing::info!("=================================================");
            tracing::info!("[SUCCESS] All integration test steps passed!");
            tracing::info!("=================================================");

            Ok(())
        })
    })
    .await?;

    Ok(())
}

#[tokio::test]
async fn test_build_dockerfile_memcached() -> Result<()> {
    tracing_subscriber_init();

    // ===== Resolve local paths =====
    let rkforge_bin = get_rkforge_binary_path()?;
    let docker_dir = get_memcached_docker_dir()?;

    tracing::info!("Using rkforge binary at {:?}", rkforge_bin);
    tracing::info!("Using memcached-docker dir at {:?}", docker_dir);

    // ===== Create VM =====
    let image = create_image(Distro::Debian, "debian-13-generic-amd64").await?;
    let config = MachineConfig {
        core: 4,
        mem: 4096,
        disk: Some(20),
        clear: true,
    };

    with_machine(&image, &config, |vm| {
        Box::pin(async move {
            tracing::info!("VM started successfully");

            // ===== Step 1: Install dependencies =====
            install_deps(vm).await?;

            // ===== Step 2: Upload rkforge binary =====
            tracing::info!("Uploading rkforge binary...");
            vm.upload(&rkforge_bin, Path::new("/usr/local/bin")).await?;
            exec_show(
                vm,
                "Make rkforge executable",
                "chmod +x /usr/local/bin/rkforge",
            )
            .await?;
            exec_show(vm, "Verify rkforge binary", "file /usr/local/bin/rkforge").await?;

            // ===== Step 3: Upload test Dockerfile directory =====
            tracing::info!("Uploading memcached-docker test directory...");
            vm.create_dir_all(Path::new("/test")).await?;
            vm.upload(&docker_dir, Path::new("/test")).await?;

            // ===== Step 3.5: Make all .sh files executable =====
            exec_show(
                vm,
                "Make shell scripts executable",
                "find /test -name '*.sh' -exec chmod +x {} +",
            )
            .await?;

            // ===== Step 4: List uploaded files for verification =====
            exec_show(vm, "List test files", "ls -laR /test/").await?;

            // ===== Step 5: Run rkforge build =====
            exec_show(
                vm,
                "Build memcached image with rkforge",
                "cd /test/memcached-docker && sudo /usr/local/bin/rkforge build \
                    -f /test/memcached-docker/Dockerfile \
                    -t memcached-test \
                    -o /test/output \
                    /test/memcached-docker",
            )
            .await?;

            // ===== Step 6: Verify build output =====
            exec_show(vm, "List build output", "ls -laR /test/output/").await?;

            // ===== Step 7: Load OCI layout into podman using skopeo =====
            exec_show(
                vm,
                "Load OCI layout into podman",
                "skopeo copy oci:/test/output/memcached-test containers-storage:localhost/memcached-test",
            )
            .await?;

            // ===== Step 8: List and verify images =====
            exec_show(vm, "List podman images", "podman images").await?;

            // ===== Step 9: Run memcached container =====
            exec_show(
                vm,
                "Run memcached container",
                "podman run -d --name memcached-test -p 11211:11211 localhost/memcached-test",
            )
            .await?;

            // Wait for container to start up
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;

            // ===== Step 10: Check container status =====
            exec_show(vm, "Check container status", "podman ps -a").await?;
            exec_show(
                vm,
                "Check container logs",
                "podman logs memcached-test 2>&1",
            )
            .await?;

            // ===== Step 11: Verify memcached is running =====
            let logs_output = exec_show(
                vm,
                "Verify memcached logs",
                "podman logs memcached-test 2>&1",
            )
            .await?;

            // Verify the logs contain memcached startup indicators
            if logs_output.to_lowercase().contains("memcached")
                || logs_output.contains("listening")
                || logs_output.contains("started")
            {
                println!("\n========================================");
                println!("[SUCCESS] Memcached container is running correctly!");
                println!("========================================");
                tracing::info!("[SUCCESS] Memcached container is running correctly!");
            } else {
                // Check if container is running even if logs don't contain expected strings
                let ps_output = exec_show(
                    vm,
                    "Double check container is running",
                    "podman inspect memcached-test --format='{{.State.Status}}'",
                )
                .await?;

                if ps_output.trim().contains("running") {
                    println!("\n========================================");
                    println!("[SUCCESS] Memcached container is running!");
                    println!("========================================");
                    tracing::info!("[SUCCESS] Memcached container is running!");
                } else {
                    anyhow::bail!(
                        "Memcached container is not running properly. Logs:\n{}",
                        logs_output
                    );
                }
            }

            // ===== Step 12: Cleanup =====
            exec_show(vm, "Stop container", "podman stop memcached-test || true").await?;
            exec_show(vm, "Remove container", "podman rm memcached-test || true").await?;
            exec_show(vm, "Remove image", "podman rmi localhost/memcached-test || true").await?;
            exec_show(vm, "Clean up test files", "rm -rf /test || true").await?;

            tracing::info!("=================================================");
            tracing::info!("[SUCCESS] All integration test steps passed!");
            tracing::info!("=================================================");

            Ok(())
        })
    })
    .await?;

    Ok(())
}

#[tokio::test]
async fn test_build_dockerfile_mysql() -> Result<()> {
    tracing_subscriber_init();

    // ===== Resolve local paths =====
    let rkforge_bin = get_rkforge_binary_path()?;
    let docker_dir = get_mysql_docker_dir()?;

    tracing::info!("Using rkforge binary at {:?}", rkforge_bin);
    tracing::info!("Using mysql-docker dir at {:?}", docker_dir);

    // ===== Create VM =====
    let image = create_image(Distro::Debian, "debian-13-generic-amd64").await?;
    let config = MachineConfig {
        core: 4,
        mem: 4096,
        disk: Some(20),
        clear: true,
    };

    with_machine(&image, &config, |vm| {
        Box::pin(async move {
            tracing::info!("VM started successfully");

            // ===== Step 1: Install dependencies =====
            install_deps(vm).await?;

            // ===== Step 2: Upload rkforge binary =====
            tracing::info!("Uploading rkforge binary...");
            vm.upload(&rkforge_bin, Path::new("/usr/local/bin")).await?;
            exec_show(
                vm,
                "Make rkforge executable",
                "chmod +x /usr/local/bin/rkforge",
            )
            .await?;
            exec_show(vm, "Verify rkforge binary", "file /usr/local/bin/rkforge").await?;

            // ===== Step 3: Upload test Dockerfile directory =====
            tracing::info!("Uploading mysql-docker test directory...");
            vm.create_dir_all(Path::new("/test")).await?;
            vm.upload(&docker_dir, Path::new("/test")).await?;

            // ===== Step 3.5: Make all .sh files executable =====
            exec_show(
                vm,
                "Make shell scripts executable",
                "find /test -name '*.sh' -exec chmod +x {} +",
            )
            .await?;

            // ===== Step 4: List uploaded files for verification =====
            exec_show(vm, "List test files", "ls -laR /test/").await?;

            // ===== Step 5: Run rkforge build =====
            exec_show(
                vm,
                "Build mysql image with rkforge",
                "cd /test/mysql-docker && sudo /usr/local/bin/rkforge build \
                    -f /test/mysql-docker/Dockerfile.debian \
                    -t mysql-test \
                    -o /test/output \
                    /test/mysql-docker",
            )
            .await?;

            // ===== Step 6: Verify build output =====
            exec_show(vm, "List build output", "ls -laR /test/output/").await?;

            // ===== Step 7: Load OCI layout into podman using skopeo =====
            exec_show(
                vm,
                "Load OCI layout into podman",
                "skopeo copy oci:/test/output/mysql-test containers-storage:localhost/mysql-test",
            )
            .await?;

            // ===== Step 8: List and verify images =====
            exec_show(vm, "List podman images", "podman images").await?;

            // ===== Step 9: Run mysql container =====
            exec_show(
                vm,
                "Run mysql container",
                "podman run -d --name mysql-test -p 3306:3306 -e MYSQL_ROOT_PASSWORD=test123 localhost/mysql-test",
            )
            .await?;

            // Wait for MySQL to start up (may take longer)
            tokio::time::sleep(std::time::Duration::from_secs(15)).await;

            // ===== Step 10: Check container status =====
            exec_show(vm, "Check container status", "podman ps -a").await?;
            exec_show(
                vm,
                "Check container logs",
                "podman logs mysql-test 2>&1",
            )
            .await?;

            // ===== Step 11: Verify mysql is running =====
            let logs_output = exec_show(
                vm,
                "Verify mysql logs",
                "podman logs mysql-test 2>&1",
            )
            .await?;

            // Verify the logs contain mysql startup indicators
            if logs_output.to_lowercase().contains("mysql")
                || logs_output.contains("ready")
                || logs_output.contains("listening")
                || logs_output.contains("mysqld: ready for connections")
            {
                println!("\n========================================");
                println!("[SUCCESS] MySQL container is running correctly!");
                println!("========================================");
                tracing::info!("[SUCCESS] MySQL container is running correctly!");
            } else {
                // Check if container is running even if logs don't contain expected strings
                let ps_output = exec_show(
                    vm,
                    "Double check container is running",
                    "podman inspect mysql-test --format='{{.State.Status}}'",
                )
                .await?;

                if ps_output.trim().contains("running") {
                    println!("\n========================================");
                    println!("[SUCCESS] MySQL container is running!");
                    println!("========================================");
                    tracing::info!("[SUCCESS] MySQL container is running!");
                } else {
                    anyhow::bail!(
                        "MySQL container is not running properly. Logs:\n{}",
                        logs_output
                    );
                }
            }

            // ===== Step 12: Cleanup =====
            exec_show(vm, "Stop container", "podman stop mysql-test || true").await?;
            exec_show(vm, "Remove container", "podman rm mysql-test || true").await?;
            exec_show(vm, "Remove image", "podman rmi localhost/mysql-test || true").await?;
            exec_show(vm, "Clean up test files", "rm -rf /test || true").await?;

            tracing::info!("=================================================");
            tracing::info!("[SUCCESS] All integration test steps passed!");
            tracing::info!("=================================================");

            Ok(())
        })
    })
    .await?;

    Ok(())
}

#[tokio::test]
async fn test_build_dockerfile_redis() -> Result<()> {
    tracing_subscriber_init();

    // ===== Resolve local paths =====
    let rkforge_bin = get_rkforge_binary_path()?;
    let docker_dir = get_redis_docker_dir()?;

    tracing::info!("Using rkforge binary at {:?}", rkforge_bin);
    tracing::info!("Using redis-docker dir at {:?}", docker_dir);

    // ===== Create VM =====
    let image = create_image(Distro::Debian, "debian-13-generic-amd64").await?;
    let config = MachineConfig {
        core: 4,
        mem: 4096,
        disk: Some(20),
        clear: true,
    };

    with_machine(&image, &config, |vm| {
        Box::pin(async move {
            tracing::info!("VM started successfully");

            // ===== Step 1: Install dependencies =====
            install_deps(vm).await?;

            // ===== Step 2: Upload rkforge binary =====
            tracing::info!("Uploading rkforge binary...");
            vm.upload(&rkforge_bin, Path::new("/usr/local/bin")).await?;
            exec_show(
                vm,
                "Make rkforge executable",
                "chmod +x /usr/local/bin/rkforge",
            )
            .await?;
            exec_show(vm, "Verify rkforge binary", "file /usr/local/bin/rkforge").await?;

            // ===== Step 3: Upload test Dockerfile directory =====
            tracing::info!("Uploading redis-docker test directory...");
            vm.create_dir_all(Path::new("/test")).await?;
            vm.upload(&docker_dir, Path::new("/test")).await?;

            // ===== Step 3.5: Make all .sh files executable =====
            exec_show(
                vm,
                "Make shell scripts executable",
                "find /test -name '*.sh' -exec chmod +x {} +",
            )
            .await?;

            // ===== Step 4: List uploaded files for verification =====
            exec_show(vm, "List test files", "ls -laR /test/").await?;

            // ===== Step 5: Run rkforge build =====
            exec_show(
                vm,
                "Build redis image with rkforge",
                "cd /test/redis-docker && sudo /usr/local/bin/rkforge build \
                    -f /test/redis-docker/Dockerfile \
                    -t redis-test \
                    -o /test/output \
                    /test/redis-docker",
            )
            .await?;

            // ===== Step 6: Verify build output =====
            exec_show(vm, "List build output", "ls -laR /test/output/").await?;

            // ===== Step 7: Load OCI layout into podman using skopeo =====
            exec_show(
                vm,
                "Load OCI layout into podman",
                "skopeo copy oci:/test/output/redis-test containers-storage:localhost/redis-test",
            )
            .await?;

            // ===== Step 8: List and verify images =====
            exec_show(vm, "List podman images", "podman images").await?;

            // ===== Step 9: Run redis container =====
            exec_show(
                vm,
                "Run redis container",
                "podman run -d --name redis-test -p 6379:6379 localhost/redis-test",
            )
            .await?;

            // Wait for container to start up
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;

            // ===== Step 10: Check container status =====
            exec_show(vm, "Check container status", "podman ps -a").await?;
            exec_show(vm, "Check container logs", "podman logs redis-test 2>&1").await?;

            // ===== Step 11: Verify redis is running =====
            let logs_output =
                exec_show(vm, "Verify redis logs", "podman logs redis-test 2>&1").await?;

            // Verify the logs contain redis startup indicators
            if logs_output.to_lowercase().contains("redis")
                || logs_output.contains("ready")
                || logs_output.contains("listening")
            {
                println!("\n========================================");
                println!("[SUCCESS] Redis container is running correctly!");
                println!("========================================");
                tracing::info!("[SUCCESS] Redis container is running correctly!");
            } else {
                // Check if container is running even if logs don't contain expected strings
                let ps_output = exec_show(
                    vm,
                    "Double check container is running",
                    "podman inspect redis-test --format='{{.State.Status}}'",
                )
                .await?;

                if ps_output.trim().contains("running") {
                    println!("\n========================================");
                    println!("[SUCCESS] Redis container is running!");
                    println!("========================================");
                    tracing::info!("[SUCCESS] Redis container is running!");
                } else {
                    anyhow::bail!(
                        "Redis container is not running properly. Logs:\n{}",
                        logs_output
                    );
                }
            }

            // ===== Step 12: Cleanup =====
            exec_show(vm, "Stop container", "podman stop redis-test || true").await?;
            exec_show(vm, "Remove container", "podman rm redis-test || true").await?;
            exec_show(
                vm,
                "Remove image",
                "podman rmi localhost/redis-test || true",
            )
            .await?;
            exec_show(vm, "Clean up test files", "rm -rf /test || true").await?;

            tracing::info!("=================================================");
            tracing::info!("[SUCCESS] All integration test steps passed!");
            tracing::info!("=================================================");

            Ok(())
        })
    })
    .await?;

    Ok(())
}

#[tokio::test]
async fn test_build_dockerfile_postgres() -> Result<()> {
    tracing_subscriber_init();

    // ===== Resolve local paths =====
    let rkforge_bin = get_rkforge_binary_path()?;
    let docker_dir = get_postgres_docker_dir()?;

    tracing::info!("Using rkforge binary at {:?}", rkforge_bin);
    tracing::info!("Using postgres-docker dir at {:?}", docker_dir);

    // ===== Create VM =====
    let image = create_image(Distro::Debian, "debian-13-generic-amd64").await?;
    let config = MachineConfig {
        core: 4,
        mem: 4096,
        disk: Some(20),
        clear: true,
    };

    with_machine(&image, &config, |vm| {
        Box::pin(async move {
            tracing::info!("VM started successfully");

            // ===== Step 1: Install dependencies =====
            install_deps(vm).await?;

            // ===== Step 2: Upload rkforge binary =====
            tracing::info!("Uploading rkforge binary...");
            vm.upload(&rkforge_bin, Path::new("/usr/local/bin")).await?;
            exec_show(
                vm,
                "Make rkforge executable",
                "chmod +x /usr/local/bin/rkforge",
            )
            .await?;
            exec_show(vm, "Verify rkforge binary", "file /usr/local/bin/rkforge").await?;

            // ===== Step 3: Upload test Dockerfile directory =====
            tracing::info!("Uploading postgres-docker test directory...");
            vm.create_dir_all(Path::new("/test")).await?;
            vm.upload(&docker_dir, Path::new("/test")).await?;

            // ===== Step 3.5: Make all .sh files executable =====
            exec_show(
                vm,
                "Make shell scripts executable",
                "find /test -name '*.sh' -exec chmod +x {} +",
            )
            .await?;

            // ===== Step 4: List uploaded files for verification =====
            exec_show(vm, "List test files", "ls -laR /test/").await?;

            // ===== Step 5: Run rkforge build =====
            exec_show(
                vm,
                "Build postgres image with rkforge",
                "cd /test/postgres-docker && sudo /usr/local/bin/rkforge build \
                    -f /test/postgres-docker/Dockerfile \
                    -t postgres-test \
                    -o /test/output \
                    /test/postgres-docker",
            )
            .await?;

            // ===== Step 6: Verify build output =====
            exec_show(vm, "List build output", "ls -laR /test/output/").await?;

            // ===== Step 7: Load OCI layout into podman using skopeo =====
            exec_show(
                vm,
                "Load OCI layout into podman",
                "skopeo copy oci:/test/output/postgres-test containers-storage:localhost/postgres-test",
            )
            .await?;

            // ===== Step 8: List and verify images =====
            exec_show(vm, "List podman images", "podman images").await?;

            // ===== Step 9: Run postgres container =====
            exec_show(
                vm,
                "Run postgres container",
                "podman run -d --name postgres-test -p 5432:5432 -e POSTGRES_PASSWORD=test123 localhost/postgres-test",
            )
            .await?;

            // Wait for postgres to start up
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;

            // ===== Step 10: Check container status =====
            exec_show(vm, "Check container status", "podman ps -a").await?;
            exec_show(
                vm,
                "Check container logs",
                "podman logs postgres-test 2>&1",
            )
            .await?;

            // ===== Step 11: Verify postgres is running =====
            let logs_output = exec_show(
                vm,
                "Verify postgres logs",
                "podman logs postgres-test 2>&1",
            )
            .await?;

            // Verify the logs contain postgres startup indicators
            if logs_output.to_lowercase().contains("postgresql")
                || logs_output.contains("database system is ready to accept connections")
                || logs_output.contains("PostgreSQL init process complete")
            {
                println!("\n========================================");
                println!("[SUCCESS] PostgreSQL container is running correctly!");
                println!("========================================");
                tracing::info!("[SUCCESS] PostgreSQL container is running correctly!");
            } else {
                // Check if container is running even if logs don't contain expected strings
                let ps_output = exec_show(
                    vm,
                    "Double check container is running",
                    "podman inspect postgres-test --format='{{.State.Status}}'",
                )
                .await?;

                if ps_output.trim().contains("running") {
                    println!("\n========================================");
                    println!("[SUCCESS] PostgreSQL container is running!");
                    println!("========================================");
                    tracing::info!("[SUCCESS] PostgreSQL container is running!");
                } else {
                    anyhow::bail!(
                        "PostgreSQL container is not running properly. Logs:\n{}",
                        logs_output
                    );
                }
            }

            // ===== Step 12: Cleanup =====
            exec_show(vm, "Stop container", "podman stop postgres-test || true").await?;
            exec_show(vm, "Remove container", "podman rm postgres-test || true").await?;
            exec_show(vm, "Remove image", "podman rmi localhost/postgres-test || true").await?;
            exec_show(vm, "Clean up test files", "rm -rf /test || true").await?;

            tracing::info!("=================================================");
            tracing::info!("[SUCCESS] All integration test steps passed!");
            tracing::info!("=================================================");

            Ok(())
        })
    })
    .await?;

    Ok(())
}
