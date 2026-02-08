#[cfg(target_os = "linux")]
use anyhow::{Context, Result};
#[cfg(target_os = "linux")]
use qlean::{Distro, Machine, MachineConfig, create_image, with_machine};
#[cfg(target_os = "linux")]
use std::path::{Path, PathBuf};
#[cfg(target_os = "linux")]
use std::sync::Once;

#[cfg(target_os = "linux")]
const HARNESS_BIN_IN_VM: &str = "/usr/local/bin/pki_harness";

#[cfg(target_os = "linux")]
fn tracing_subscriber_init() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .init();
    });
}

#[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
fn get_harness_binary_path() -> Result<PathBuf> {
    // Check for explicit env var first
    if let Ok(path) = std::env::var("PKI_HARNESS_BIN") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Ok(path);
        }
    }

    // Try to find in target directory
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").context("CARGO_MANIFEST_DIR not set")?;

    // Assuming we are in project/libvault
    // Target dir could be ../target or ./target depending on workspace config
    // The safest way is usually looking up relative to manifest or using CARGO_TARGET_DIR

    let target_dir = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            // Heuristic: check if ../../target exists (workspace root)
            let workspace_target = PathBuf::from(&manifest_dir)
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("target");
            if workspace_target.exists() {
                workspace_target
            } else {
                PathBuf::from(&manifest_dir).join("target")
            }
        });

    // We look for the binary. Note: On macOS host, this might find a Mach-O binary
    // which won't run in Linux VM unless we cross-compiled.
    // For this test to pass in this environment, we assume the user has handled cross-compilation
    // OR we are just setting up the structure.
    //
    // Standard cargo build output:
    let debug_path = target_dir.join("debug/pki_harness");
    if debug_path.exists() {
        return Ok(debug_path);
    }

    // Also check x86_64-unknown-linux-gnu subfolder if cross-compiling
    let linux_debug_path = target_dir.join("x86_64-unknown-linux-gnu/debug/pki_harness");
    if linux_debug_path.exists() {
        return Ok(linux_debug_path);
    }

    anyhow::bail!(
        "pki_harness binary not found at {:?} or {:?}. Please build it first (e.g. `cargo build --bin pki_harness`).",
        debug_path,
        linux_debug_path
    );
}

#[cfg(target_os = "linux")]
async fn run_in_vm(vm: &mut Machine, harness_bin: &Path) -> Result<()> {
    // 1. Upload harness
    vm.upload(harness_bin, Path::new("/usr/local/bin")).await?;
    exec_check(vm, &format!("chmod +x {}", HARNESS_BIN_IN_VM)).await?;

    // 2. Run harness
    println!("Running PKI harness in VM...");
    let output = exec_check(vm, HARNESS_BIN_IN_VM).await?;
    println!("Harness Output:\n{}", output);

    Ok(())
}

#[tokio::test]
async fn test_pki_clean_lifecycle() -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    {
        tracing_subscriber_init();

        // 1. Locate the harness binary
        // Note: In a real CI/Dev flow, you'd ensure this binary is built for the target VM arch.
        // If you are running on macOS, you must cross-compile the harness first:
        // `cargo build --bin pki_harness --target x86_64-unknown-linux-gnu`
        // and ensure the path logic in `get_harness_binary_path` can find it.
        let harness_path = get_harness_binary_path()?;
        println!("Using harness binary at: {:?}", harness_path);

        // 2. Prepare VM Image
        // Using Debian as it's standard in qlean usage
        let image = create_image(Distro::Debian, "debian-13-generic-amd64").await?;

        // 3. VM Config
        let config = MachineConfig {
            core: 2,
            mem: 2048,
            disk: Some(5), // 5GB disk
            clear: true,
        };

        // 4. Boot and Run
        with_machine(&image, &config, move |vm| {
            let harness_path = harness_path.clone();
            Box::pin(async move { run_in_vm(vm, &harness_path).await })
        })
        .await?;
    }
    #[cfg(not(target_os = "linux"))]
    println!("Skipping qlean test on non-linux OS");

    Ok(())
}
