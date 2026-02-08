use std::path::{Path, PathBuf};

use std::sync::Once;
use std::time::Duration;

use anyhow::{Context, Result};
use qlean::{Distro, Machine, MachineConfig, create_image, with_machine};
use tracing_subscriber::EnvFilter;

const SLAYERFS_BIN_IN_VM: &str = "/usr/local/bin/slayerfs";
const SLAYERFS_MOUNTPOINT: &str = "/mnt/slayerfs";
const SLAYERFS_DATA_DIR: &str = "/tmp/slayerfs-data";
const SLAYERFS_LOG_PATH: &str = "/var/log/slayerfs.log";
const SLAYERFS_META_DIR: &str = "/tmp/slayerfs-meta";

// NOTE: These tests use qlean for QEMU-based VM testing.
// qlean 0.2.1+ supports TCG fallback when KVM is unavailable.
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

async fn get_file_tail(vm: &mut Machine, path: &str, max_lines: usize) -> Result<String> {
    let out = vm
        .exec(&format!(
            "tail -n {n} {p} 2>/dev/null | tail -c 12000 || true",
            n = max_lines,
            p = path
        ))
        .await?;
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

async fn dump_debug_info(vm: &mut Machine) -> Result<String> {
    let mut s = String::new();

    s.push_str("=== mount ===\n");
    s.push_str(&exec_check(vm, "mount | tail -n 50 || true").await?);

    s.push_str("\n=== mountpoint ===\n");
    s.push_str(
        &exec_check(
            vm,
            &format!(
                "mountpoint -q {mp} && echo 'mounted' || echo 'not mounted'",
                mp = SLAYERFS_MOUNTPOINT
            ),
        )
        .await?,
    );
    s.push('\n');

    s.push_str("\n=== ps (slayerfs) ===\n");
    s.push_str(&exec_check(vm, "ps aux | grep '[s]layerfs' || true").await?);

    s.push_str("\n=== logs (slayerfs) ===\n");
    s.push_str(&get_file_tail(vm, SLAYERFS_LOG_PATH, 300).await?);

    s.push_str("\n=== sqlite dir ===\n");
    s.push_str(&exec_check(vm, "ls -la /tmp/slayerfs-db 2>/dev/null || true").await?);
    s.push_str(&exec_check(vm, "stat /tmp/slayerfs-db/metadata.db 2>/dev/null || true").await?);

    s.push_str("\n=== dmesg (tail) ===\n");
    s.push_str(&exec_check(vm, "dmesg -T | tail -n 80 || true").await?);

    Ok(s)
}

fn get_slayerfs_binary_path() -> Result<PathBuf> {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_slayerfs") {
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

    let debug_path = target_dir.join("debug/slayerfs");
    if debug_path.exists() {
        return Ok(debug_path);
    }

    let release_path = target_dir.join("release/slayerfs");
    if release_path.exists() {
        return Ok(release_path);
    }

    anyhow::bail!(
        "slayerfs binary not found at {:?} or {:?}. Build it first, e.g. `cargo build -p slayerfs`.",
        debug_path,
        release_path
    );
}

async fn install_deps(vm: &mut Machine) -> Result<()> {
    exec_check(vm, "apt-get update -qq").await?;
    exec_check(
        vm,
        "DEBIAN_FRONTEND=noninteractive apt-get install -y -qq fuse3 ca-certificates coreutils util-linux procps",
    )
    .await?;

    exec_check(
        vm,
        &format!(
            "mkdir -p {mp} {data}",
            mp = SLAYERFS_MOUNTPOINT,
            data = SLAYERFS_DATA_DIR
        ),
    )
    .await?;

    exec_check(
        vm,
        "(grep -q '^user_allow_other' /etc/fuse.conf 2>/dev/null) || echo 'user_allow_other' >> /etc/fuse.conf",
    )
    .await?;

    Ok(())
}

async fn wait_for_mount(vm: &mut Machine, timeout: Duration) -> Result<()> {
    let start = std::time::Instant::now();
    loop {
        let out = vm
            .exec(&format!(
                "mountpoint -q {mp} && echo OK || echo NO",
                mp = SLAYERFS_MOUNTPOINT
            ))
            .await?;
        let s = String::from_utf8_lossy(&out.stdout);
        if s.trim() == "OK" {
            return Ok(());
        }

        if start.elapsed() > timeout {
            let ps = vm
                .exec("ps aux | grep '[s]layerfs' || true")
                .await
                .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                .unwrap_or_else(|e| e.to_string());
            let log = get_file_tail(vm, SLAYERFS_LOG_PATH, 800)
                .await
                .unwrap_or_else(|e| e.to_string());
            anyhow::bail!(
                "timeout waiting for slayerfs mount at {}\n\n=== ps (slayerfs) ===\n{}\n=== logs (slayerfs) ===\n{}",
                SLAYERFS_MOUNTPOINT,
                ps,
                log
            );
        }

        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

async fn unmount_best_effort(vm: &mut Machine) -> Result<()> {
    let _ = vm
        .exec(&format!(
            "fusermount3 -u {mp} || umount {mp} || umount -l {mp} || true",
            mp = SLAYERFS_MOUNTPOINT
        ))
        .await;
    Ok(())
}

async fn assert_not_mounted(vm: &mut Machine) -> Result<()> {
    let out = vm
        .exec(&format!(
            "mountpoint -q {mp} && echo 'mounted' || echo 'not mounted'",
            mp = SLAYERFS_MOUNTPOINT
        ))
        .await?;
    let s = String::from_utf8_lossy(&out.stdout);
    if s.trim() != "not mounted" {
        anyhow::bail!("expected mountpoint to be unmounted, but it is still mounted");
    }
    Ok(())
}

async fn run_slayerfs_mount_root(vm: &mut Machine, meta_url: &str) -> Result<()> {
    exec_check(
        vm,
        &format!("rm -f {log} && touch {log}", log = SLAYERFS_LOG_PATH),
    )
    .await?;

    exec_check(
        vm,
        &format!("rm -rf {d} && mkdir -p {d}", d = SLAYERFS_META_DIR),
    )
    .await?;
    exec_check(vm, &format!("chmod 1777 {d}", d = SLAYERFS_META_DIR)).await?;

    exec_check(
        vm,
        &format!(
            "stat -c '%A %U:%G %n' /tmp {d} || true; sh -lc 'touch {d}/.probe_root && rm {d}/.probe_root'",
            d = SLAYERFS_META_DIR
        ),
    )
    .await?;
    exec_check(
        vm,
        &format!(
            "su - tester -c 'sh -lc \"touch {d}/.probe_tester && rm {d}/.probe_tester\"'",
            d = SLAYERFS_META_DIR
        ),
    )
    .await?;

    let db_path = meta_url
        .strip_prefix("sqlite:///")
        .or_else(|| meta_url.strip_prefix("sqlite://"))
        .map(|s| s.split('?').next().unwrap_or(s))
        .filter(|s| !s.starts_with(':'))
        .map(|p| {
            if p.starts_with('/') {
                p.to_string()
            } else {
                format!("/{p}")
            }
        });
    if let Some(db_path) = db_path {
        exec_check(
            vm,
            &format!(
                "sh -lc \"test -e '{p}' || touch '{p}'; chmod 666 '{p}'; stat -c '%A %U:%G %n' '{p}'\"",
                p = db_path
            ),
        )
        .await?;
    }

    exec_check(
        vm,
        &format!(
            "nohup {bin} mount {mp} --data-dir {data} --meta-backend sqlx --meta-url '{meta_url}' > {log} 2>&1 &",
            bin = SLAYERFS_BIN_IN_VM,
            mp = SLAYERFS_MOUNTPOINT,
            data = SLAYERFS_DATA_DIR,
            meta_url = meta_url,
            log = SLAYERFS_LOG_PATH
        ),
    )
    .await?;

    wait_for_mount(vm, Duration::from_secs(30)).await?;
    Ok(())
}

async fn basic_fs_checks(vm: &mut Machine) -> Result<()> {
    exec_check(
        vm,
        &format!("sh -lc 'echo hello > {}/a'", SLAYERFS_MOUNTPOINT),
    )
    .await?;
    exec_check(
        vm,
        &format!("sh -lc 'cat {}/a | grep -q hello'", SLAYERFS_MOUNTPOINT),
    )
    .await?;
    exec_check(
        vm,
        &format!("mv {}/a {}/b", SLAYERFS_MOUNTPOINT, SLAYERFS_MOUNTPOINT),
    )
    .await?;
    exec_check(vm, &format!("rm {}/b", SLAYERFS_MOUNTPOINT)).await?;

    exec_check(vm, &format!("mkdir {}/dir", SLAYERFS_MOUNTPOINT)).await?;
    exec_check(vm, &format!("rmdir {}/dir", SLAYERFS_MOUNTPOINT)).await?;

    Ok(())
}

async fn setup_unprivileged_user(vm: &mut Machine, user: &str) -> Result<()> {
    exec_check(
        vm,
        &format!("id -u {user} >/dev/null 2>&1 || useradd -m -s /bin/bash {user}"),
    )
    .await?;
    Ok(())
}

async fn run_posix_edge_cases(vm: &mut Machine) -> Result<()> {
    exec_check(
        vm,
        &format!(
            "mkdir -p {}/edge/dir1 {}/edge/dir2",
            SLAYERFS_MOUNTPOINT, SLAYERFS_MOUNTPOINT
        ),
    )
    .await?;

    exec_check(
        vm,
        &format!(
            "sh -lc 'echo original > {}/edge/dir1/file'",
            SLAYERFS_MOUNTPOINT
        ),
    )
    .await?;

    exec_check(
        vm,
        &format!(
            "ln {}/edge/dir1/file {}/edge/dir1/hardlink",
            SLAYERFS_MOUNTPOINT, SLAYERFS_MOUNTPOINT
        ),
    )
    .await?;
    exec_check(
        vm,
        &format!(
            "sh -lc 'test $(stat -c %h {}/edge/dir1/file) -eq 2'",
            SLAYERFS_MOUNTPOINT
        ),
    )
    .await?;
    exec_check(
        vm,
        &format!(
            "sh -lc 'echo via-link >> {}/edge/dir1/hardlink'",
            SLAYERFS_MOUNTPOINT
        ),
    )
    .await?;
    exec_check(
        vm,
        &format!(
            "sh -lc 'grep -q via-link {}/edge/dir1/file'",
            SLAYERFS_MOUNTPOINT
        ),
    )
    .await?;

    exec_check(
        vm,
        &format!(
            "ln -s {}/edge/dir1/file {}/edge/dir2/symlink",
            SLAYERFS_MOUNTPOINT, SLAYERFS_MOUNTPOINT
        ),
    )
    .await?;
    exec_check(
        vm,
        &format!(
            "sh -lc 'cat {}/edge/dir2/symlink | grep -q original'",
            SLAYERFS_MOUNTPOINT
        ),
    )
    .await?;

    exec_check(
        vm,
        &format!(
            "mv {}/edge/dir1/file {}/edge/dir2/moved",
            SLAYERFS_MOUNTPOINT, SLAYERFS_MOUNTPOINT
        ),
    )
    .await?;
    exec_check(
        vm,
        &format!("test -f {}/edge/dir2/moved", SLAYERFS_MOUNTPOINT),
    )
    .await?;
    exec_check(
        vm,
        &format!("test ! -f {}/edge/dir1/file", SLAYERFS_MOUNTPOINT),
    )
    .await?;

    Ok(())
}

async fn run_concurrency_smoke(vm: &mut Machine) -> Result<()> {
    exec_check(vm, &format!("mkdir -p {}/concurrency", SLAYERFS_MOUNTPOINT)).await?;

    exec_check(
        vm,
        &format!(
            "sh -lc 'set -e; rm -f {}/concurrency/file-*; \
for i in $(seq 1 4); do \
  for j in $(seq 1 50); do echo \"$i-$j\" >> {}/concurrency/file-$i; done; \
done'",
            SLAYERFS_MOUNTPOINT, SLAYERFS_MOUNTPOINT
        ),
    )
    .await?;

    let total1 = exec_check(
        vm,
        &format!("sh -lc 'wc -l {}/concurrency/file-*'", SLAYERFS_MOUNTPOINT),
    )
    .await?;

    let expected_per_file = 50usize;
    let expected_total = 4usize * expected_per_file;

    let mut seen_paths = std::collections::BTreeSet::new();
    let mut total_lines = 0usize;

    for line in total1.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(count) = parts.next() else {
            continue;
        };
        let Some(path) = parts.next() else {
            continue;
        };
        if path == "total" {
            continue;
        }

        let count: usize = count
            .parse()
            .with_context(|| format!("failed to parse per-file wc output: '{}'", line))?;

        if count != expected_per_file {
            anyhow::bail!(
                "unexpected per-file line count for {}: got {}, expected {}\n\nper-file wc output:\n{}",
                path,
                count,
                expected_per_file,
                total1
            );
        }

        if seen_paths.insert(path.to_string()) {
            total_lines += count;
        }
    }

    if seen_paths.len() != 4 {
        anyhow::bail!(
            "unexpected number of concurrency files: got {}, expected 4\n\nper-file wc output:\n{}",
            seen_paths.len(),
            total1
        );
    }

    if total_lines != expected_total {
        anyhow::bail!(
            "unexpected merged line count (sum of unique files): got {}, expected {}\n\nper-file wc output:\n{}",
            total_lines,
            expected_total,
            total1
        );
    }

    Ok(())
}

async fn kill_slayerfs_best_effort(vm: &mut Machine) -> Result<()> {
    let _ = vm.exec("pkill -TERM slayerfs >/dev/null 2>&1 || true; sleep 1; pkill -KILL slayerfs >/dev/null 2>&1 || true").await;
    Ok(())
}

async fn run_in_vm(vm: &mut Machine, slayerfs_bin: &Path) -> Result<()> {
    let r = run_full(vm, slayerfs_bin).await;

    if let Err(e) = r {
        let dbg = dump_debug_info(vm).await.unwrap_or_else(|e| e.to_string());
        let _ = unmount_best_effort(vm).await;
        anyhow::bail!("{}\n\n{}", e, dbg);
    }

    Ok(())
}

async fn start_vm_and_run(slayerfs_bin: PathBuf) -> Result<()> {
    tracing_subscriber_init();

    tracing::info!("using slayerfs binary at {:?}", slayerfs_bin);

    let image = create_image(Distro::Debian, "debian-13-generic-amd64").await?;
    let config = MachineConfig {
        core: 2,
        mem: 2048,
        disk: Some(10),
        clear: true,
    };

    let slayerfs_bin = std::sync::Arc::new(slayerfs_bin);
    with_machine(&image, &config, move |vm| {
        let slayerfs_bin = std::sync::Arc::clone(&slayerfs_bin);
        Box::pin(async move { run_in_vm(vm, slayerfs_bin.as_ref()).await })
    })
    .await?;

    Ok(())
}

async fn run_full(vm: &mut Machine, slayerfs_bin: &Path) -> Result<()> {
    install_deps(vm).await?;
    setup_unprivileged_user(vm, "tester").await?;
    upload_slayerfs(vm, slayerfs_bin).await?;

    let db_dir = "/tmp/slayerfs-db";
    let db_path = "/tmp/slayerfs-db/metadata.db";
    exec_check(vm, &format!("rm -rf {db_dir} && mkdir -p {db_dir}")).await?;
    exec_check(
        vm,
        &format!(
            "stat -c '%A %U:%G %n' /tmp {db_dir} || true; ls -ld {db_dir} && sh -lc 'touch {db_dir}/.w && rm {db_dir}/.w'",
        ),
    )
    .await?;

    exec_check(
        vm,
        &format!("rm -f {db_path} && touch {db_path} && stat -c '%A %U:%G %n' {db_path}",),
    )
    .await?;

    let meta_url = format!("sqlite://{db_path}");
    run_slayerfs_mount_root(vm, &meta_url).await?;
    basic_fs_checks(vm).await?;

    exec_check(
        vm,
        &format!(
            "sh -lc 'echo persist > {}/persist.txt'",
            SLAYERFS_MOUNTPOINT
        ),
    )
    .await?;
    exec_check(vm, &format!("mkdir -p {}/pdir/sub", SLAYERFS_MOUNTPOINT)).await?;

    // Ensure data is flushed to storage before killing the process
    exec_check(vm, "sync").await?;
    tokio::time::sleep(Duration::from_secs(2)).await;

    kill_slayerfs_best_effort(vm).await?;
    unmount_best_effort(vm).await?;
    assert_not_mounted(vm).await?;

    run_slayerfs_mount_root(vm, &meta_url).await?;
    exec_check(
        vm,
        &format!(
            "sh -lc 'cat {}/persist.txt | grep -q persist'",
            SLAYERFS_MOUNTPOINT
        ),
    )
    .await?;
    exec_check(vm, &format!("test -d {}/pdir/sub", SLAYERFS_MOUNTPOINT)).await?;

    run_posix_edge_cases(vm).await?;
    run_concurrency_smoke(vm).await?;

    kill_slayerfs_best_effort(vm).await?;
    unmount_best_effort(vm).await?;
    assert_not_mounted(vm).await?;

    Ok(())
}

async fn upload_slayerfs(vm: &mut Machine, slayerfs_bin: &Path) -> Result<()> {
    vm.upload(slayerfs_bin, Path::new("/usr/local/bin")).await?;
    exec_check(vm, &format!("chmod +x {}", SLAYERFS_BIN_IN_VM)).await?;
    Ok(())
}

#[tokio::test]
#[ignore]
async fn test_slayerfs_kvm() -> Result<()> {
    let slayerfs_bin = get_slayerfs_binary_path()?;
    start_vm_and_run(slayerfs_bin).await
}
