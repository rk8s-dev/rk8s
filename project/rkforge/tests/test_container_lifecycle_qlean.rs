//! Container lifecycle integration tests using qlean VMs.
//!
//! These tests run inside a QEMU/KVM virtual machine using the qlean crate.
//! NOTE: qlean 0.2.2+ supports TCG fallback when KVM is unavailable,
//! so these tests can run on GitHub Actions runners without nested virtualization.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Ok, Result, bail};

use qlean::{Distro, MachineConfig, create_image, with_machine};
use serial_test::serial;
use tempfile::tempdir;

fn project_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("rkforge manifest must have a parent dir")
        .to_path_buf()
}

fn host_target_bin(project_dir: &Path, name: &str) -> PathBuf {
    project_dir.join("target").join("debug").join(name)
}

fn ensure_aardvark_dns(project_dir: &Path) -> Result<PathBuf> {
    let aardvark_path = host_target_bin(project_dir, "aardvark-dns");
    if aardvark_path.exists() {
        return Ok(aardvark_path);
    }
    let status = Command::new("cargo")
        .current_dir(project_dir)
        .args(["build", "-p", "aardvark-dns"])
        .status()
        .context("failed to run cargo build for aardvark-dns")?;
    if !status.success() {
        bail!("failed to build aardvark-dns");
    }

    if !aardvark_path.exists() {
        bail!(
            "aardvark-dns binary not found at {}",
            aardvark_path.display()
        );
    }

    Ok(aardvark_path)
}
fn container_yaml(name: &str, image: &str, args: &[&str]) -> String {
    let args_yaml = args
        .iter()
        .map(|s| format!("  - {s}"))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "name: {name}\nimage: {image}\nargs:\n{args_yaml}\nports: []\nresources: null\nliveness_probe: null\nreadiness_probe: null\nstartup_probe: null\nsecurity_context: null\nenv: null\nvolume_mounts: null\ncommand: null\nworking_dir: null\n"
    )
}

async fn vm_exec_ok(vm: &mut qlean::Machine, cmd: &str) -> Result<()> {
    let out = vm.exec(cmd).await.context(cmd.to_string())?;
    if !out.status.success() {
        bail!(
            "command failed: {cmd}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(())
}

async fn vm_assert_status(vm: &mut qlean::Machine, id: &str, expected: &str) -> Result<()> {
    let cmd = format!("/usr/local/bin/rkforge state {id}");
    let out = vm.exec(&cmd).await?;
    if !out.status.success() {
        bail!(
            "command failed: {cmd}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let stdout = String::from_utf8(out.stdout)?;
    let last = stdout.lines().last().unwrap_or("");
    let status = last
        .split_whitespace()
        .nth(2)
        .unwrap_or("")
        .to_ascii_lowercase();
    if status != expected.to_ascii_lowercase() {
        let state_json = vm
            .exec(format!(
                "cat /run/youki/{id}/state.json 2>/dev/null || true"
            ))
            .await
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();
        bail!(
            "unexpected status for {id}: {status} (expected {expected})\nstate:\n{stdout}\nstate.json:\n{state_json}"
        );
    }
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_container_lifecycle_sync_mode_with_qlean() -> Result<()> {
    let project_dir = project_dir();
    let rkforge_bin = PathBuf::from(env!("CARGO_BIN_EXE_rkforge"));
    let aardvark_path = ensure_aardvark_dns(&project_dir)?;
    let bundles_dir = project_dir.join("test").join("bundles");
    let pause_bundle = bundles_dir.join("pause");
    if !pause_bundle.exists() {
        bail!("pause bundle not found at {}", pause_bundle.display());
    }

    let tmp = tempdir()?;
    let create_id = "qlean-lifecycle-create";
    let run_id = "qlean-lifecycle-run";
    let create_yaml = tmp.path().join("create.yml");
    let run_yaml = tmp.path().join("run.yml");
    std::fs::write(
        &create_yaml,
        container_yaml(create_id, "/root/bundles/pause", &["/pause"]),
    )?;
    std::fs::write(
        &run_yaml,
        container_yaml(run_id, "/root/bundles/pause", &["/pause"]),
    )?;

    let image = create_image(Distro::Debian, "debian-13-generic-amd64").await?;
    let config = MachineConfig {
        core: 2,
        mem: 4096,
        disk: Some(20),
        clear: true,
    };

    with_machine(&image, &config, |vm| {
        Box::pin(async move {
            vm_exec_ok(
                vm,
                "mkdir -p /opt/cni/bin /etc/cni/net.d /usr/local/bin /root/bundles /root/specs",
            )
            .await?;
            vm.upload(&rkforge_bin, "/usr/local/bin").await?;
            vm.upload(&aardvark_path, "/usr/bin").await?;
            vm.upload(&pause_bundle, "/root/bundles").await?;
            vm.upload(&create_yaml, "/root/specs").await?;
            vm.upload(&run_yaml, "/root/specs").await?;

            vm_exec_ok(vm, "chmod +x /usr/local/bin/rkforge /usr/bin/aardvark-dns").await?;
            vm_exec_ok(vm, "chmod -R a+rx /root/bundles/pause/rootfs || true").await?;

            vm_exec_ok(vm, "/usr/local/bin/rkforge create /root/specs/create.yml").await?;
            vm_exec_ok(vm, &format!("test -f /run/youki/{create_id}/state.json")).await?;
            vm_assert_status(vm, create_id, "created").await?;

            vm_exec_ok(vm, &format!("/usr/local/bin/rkforge start {create_id}")).await?;
            vm_assert_status(vm, create_id, "running").await?;
            vm_exec_ok(
                vm,
                &format!(
                    "grep -Eq '\"pid\"\\s*:\\s*[1-9][0-9]*' /run/youki/{create_id}/state.json"
                ),
            )
            .await?;

            vm_exec_ok(
                vm,
                &format!("/usr/local/bin/rkforge stop {create_id} --timeout 5"),
            )
            .await?;
            vm_exec_ok(
                vm,
                &format!("/usr/local/bin/rkforge wait {create_id} --timeout 10"),
            )
            .await?;
            vm_assert_status(vm, create_id, "stopped").await?;

            vm_exec_ok(vm, &format!("/usr/local/bin/rkforge rm {create_id}")).await?;
            vm_exec_ok(vm, &format!("test ! -d /run/youki/{create_id}")).await?;
            vm_exec_ok(
                vm,
                &format!("test -z \"$(ls -d /sys/fs/cgroup/*{create_id}* 2>/dev/null || true)\""),
            )
            .await?;

            vm_exec_ok(vm, "/usr/local/bin/rkforge run /root/specs/run.yml").await?;
            vm_assert_status(vm, run_id, "running").await?;
            vm_exec_ok(
                vm,
                &format!("/usr/local/bin/rkforge kill {run_id} --signal KILL"),
            )
            .await?;
            vm_exec_ok(
                vm,
                &format!("/usr/local/bin/rkforge wait {run_id} --timeout 10"),
            )
            .await?;
            vm_assert_status(vm, run_id, "stopped").await?;

            vm_exec_ok(vm, &format!("/usr/local/bin/rkforge rm -f {run_id}")).await?;
            vm_exec_ok(vm, &format!("test ! -d /run/youki/{run_id}")).await?;

            tokio::time::sleep(Duration::from_millis(200)).await;
            vm_exec_ok(
                vm,
                &format!("test -z \"$(ls -d /sys/fs/cgroup/*{run_id}* 2>/dev/null || true)\""),
            )
            .await?;

            Ok(())
        })
    })
    .await?;

    Ok(())
}
