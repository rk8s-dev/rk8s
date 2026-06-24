use std::path::{Path, PathBuf};
use std::sync::Once;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use qlean::{Distro, Machine, MachineConfig, MachinePool, create_image, with_machine, with_pool};
use tracing_subscriber::EnvFilter;

const ROLE_CLIENT1: &str = "client1";
const ROLE_CLIENT2: &str = "client2";

const CLIENT_FUSE_MOUNT: &str = "/mnt/slayerfs";

const HOST_DIAG_ROOT: &str = "/tmp/slayerfs-qlean-multinode";
const DIST_TESTS_DIR: &str = "/opt/slayerfs-dist-tests/distributed-tests";
const DIST_RESULTS_DIR: &str = "/tmp/slayerfs-results";
const DIST_WORKDIR: &str = "/root/slayerfs-dist";
const DIST_RUN_TIMEOUT_SECS: u64 = 30 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MetaBackend {
    Etcd,
    Redis,
    Postgres,
}

impl MetaBackend {
    fn as_str(self) -> &'static str {
        match self {
            MetaBackend::Etcd => "etcd",
            MetaBackend::Redis => "redis",
            MetaBackend::Postgres => "postgres",
        }
    }
}

fn tracing_subscriber_init() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .init();
    });
}

fn host_run_id() -> String {
    let pid = std::process::id();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!(
        "{secs}-{nanos}-pid{pid}",
        secs = now.as_secs(),
        nanos = now.subsec_nanos(),
        pid = pid
    )
}

fn qlean_runs_dir() -> Option<PathBuf> {
    let base = if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        PathBuf::from(xdg)
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".local/share")
    } else {
        return None;
    };
    Some(base.join("qlean").join("runs"))
}

async fn write_host_file(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(path, contents.as_bytes()).await?;
    Ok(())
}

async fn snapshot_qlean_runs(dst_dir: &Path) -> Result<()> {
    let Some(runs_dir) = qlean_runs_dir() else {
        write_host_file(
            &dst_dir.join("qlean-runs-snapshot.txt"),
            "qlean runs dir: (unavailable: neither XDG_DATA_HOME nor HOME set)\n",
        )
        .await?;
        return Ok(());
    };

    let mut out = String::new();
    out.push_str(&format!("qlean runs dir: {}\n\n", runs_dir.display()));

    let rd = tokio::fs::read_dir(&runs_dir).await;
    let mut rd = match rd {
        Ok(rd) => rd,
        Err(e) => {
            out.push_str(&format!("failed to read runs dir: {e}\n"));
            write_host_file(&dst_dir.join("qlean-runs-snapshot.txt"), &out).await?;
            return Ok(());
        }
    };

    while let Some(ent) = rd.next_entry().await? {
        let path = ent.path();
        let name = ent.file_name();
        let name = name.to_string_lossy();

        let meta = match ent.metadata().await {
            Ok(m) => m,
            Err(e) => {
                out.push_str(&format!("- {name}: metadata error: {e}\n"));
                continue;
            }
        };
        if !meta.is_dir() {
            continue;
        }

        out.push_str(&format!("- run {name}: {}\n", path.display()));
        for f in ["cid", "qemu.pid"].iter() {
            let p = path.join(f);
            match tokio::fs::read_to_string(&p).await {
                Ok(s) => out.push_str(&format!("    {f}: {}\n", s.trim())),
                Err(_) => out.push_str(&format!("    {f}: (missing/unreadable)\n")),
            }
        }
    }

    write_host_file(&dst_dir.join("qlean-runs-snapshot.txt"), &out).await?;
    Ok(())
}

async fn cleanup_qlean_runs() {
    let Some(runs_dir) = qlean_runs_dir() else {
        return;
    };
    let _ = tokio::fs::remove_dir_all(&runs_dir).await;
}

async fn collect_host_init_failure(host_init_dir: &Path, vm_name: &str, err: &anyhow::Error) {
    let _ = write_host_file(
        &host_init_dir.join("failed-vm.txt"),
        &format!("{vm_name}\n"),
    )
    .await;
    let _ = write_host_file(
        &host_init_dir.join("error.txt"),
        &format!("init failed for vm={vm_name}\n\n{err:?}\n"),
    )
    .await;
    let _ = snapshot_qlean_runs(host_init_dir).await;
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

async fn exec_timed(vm: &mut Machine, cmd: &str, timeout: std::time::Duration) -> Result<String> {
    let result = tokio::time::timeout(timeout, vm.exec(cmd)).await;
    let result = match result {
        Ok(res) => res?,
        Err(_) => anyhow::bail!("command timed out after {:?}: {}", timeout, cmd),
    };
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

async fn exec(vm: &mut Machine, cmd: &str) -> Result<String> {
    let result = vm.exec(cmd).await?;
    let stdout = String::from_utf8_lossy(&result.stdout);
    let stderr = String::from_utf8_lossy(&result.stderr);
    Ok(format!(
        "$ {cmd}\nexit={code:?}\nstdout:\n{stdout}\nstderr:\n{stderr}\n",
        code = result.status.code(),
    ))
}

async fn maybe_run_single_vm_probe(image: &qlean::Image, config: &MachineConfig) -> Result<()> {
    let enabled = std::env::var("SLAYERFS_QLEAN_SINGLE_VM_CHECK")
        .map(|v| v == "1")
        .unwrap_or(false);
    if !enabled {
        return Ok(());
    }

    tracing::info!("running single-VM qlean probe before pool init");
    with_machine(image, config, |vm| {
        Box::pin(async move {
            exec_check(vm, "true").await?;
            Ok(())
        })
    })
    .await
    .context("single-VM qlean probe failed")?;

    Ok(())
}

async fn apt_install_with_retries(vm: &mut Machine, packages: &[&str]) -> Result<()> {
    let pkgs = packages.join(" ");
    tracing::info!(%pkgs, "apt: installing packages");

    // Cloud images sometimes have apt locks / transient mirror failures.
    // Keep it deterministic but resilient.
    let cmd = format!(
        r#"sh -lc '
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive

try=0
until [ "$try" -ge 8 ]; do
  try=$((try+1))
  if apt-get update -y -qq && apt-get install -y -qq --no-install-recommends {pkgs}; then
    exit 0
  fi
  echo "apt attempt $try failed; sleeping..." >&2
  sleep 3
done

echo "apt failed after retries" >&2
exit 1
'"#,
        pkgs = pkgs
    );
    exec_check(vm, &cmd).await?;
    Ok(())
}

async fn ensure_base_packages(vm: &mut Machine, packages: &[&str]) -> Result<()> {
    let missing: String = exec_check(
        vm,
        "sh -lc 'missing=\"\"; for cmd in curl jq fio fusermount3 mountpoint ps ip; do command -v \"$cmd\" >/dev/null 2>&1 || missing=\"$missing $cmd\"; done; echo \"$missing\"'",
    )
    .await?;
    if missing.trim().is_empty() {
        return Ok(());
    }
    apt_install_with_retries(vm, packages).await
}

async fn ensure_etcdctl(vm: &mut Machine) -> Result<()> {
    if exec_check(vm, "sh -lc 'command -v etcdctl >/dev/null 2>&1'")
        .await
        .is_ok()
    {
        return Ok(());
    }
    apt_install_with_retries(vm, &["etcd-client"]).await
}

async fn ensure_redis_cli(vm: &mut Machine) -> Result<()> {
    if exec_check(vm, "sh -lc 'command -v redis-cli >/dev/null 2>&1'")
        .await
        .is_ok()
    {
        return Ok(());
    }
    apt_install_with_retries(vm, &["redis-tools"]).await
}

async fn ensure_psql(vm: &mut Machine) -> Result<()> {
    if exec_check(vm, "sh -lc 'command -v psql >/dev/null 2>&1'")
        .await
        .is_ok()
    {
        return Ok(());
    }
    apt_install_with_retries(vm, &["postgresql-client"]).await
}

async fn ensure_vm_ssh(vm: &mut Machine) -> Result<()> {
    apt_install_with_retries(vm, &["openssh-server", "openssh-client"]).await?;
    exec_check(
        vm,
        &format!(
            "sh -lc 'mkdir -p /root/.ssh && chmod 700 /root/.ssh && systemctl enable --now ssh && mkdir -p {DIST_WORKDIR}/bin && chmod 0777 {DIST_WORKDIR} {DIST_WORKDIR}/bin'",
        ),
    )
    .await?;
    Ok(())
}

async fn configure_ssh_keypair(vm: &mut Machine) -> Result<()> {
    exec_check(
        vm,
        "sh -lc 'test -f /root/.ssh/id_ed25519 || ssh-keygen -t ed25519 -N \"\" -f /root/.ssh/id_ed25519'",
    )
    .await?;
    Ok(())
}

async fn read_vm_file(vm: &mut Machine, path: &str) -> Result<String> {
    let out = exec_check(vm, &format!("sh -lc 'cat {path}'")).await?;
    Ok(out)
}

async fn append_authorized_key(vm: &mut Machine, pubkey: &str) -> Result<()> {
    let key = pubkey.trim();
    exec_check(
        vm,
        &format!(
            r#"sh -lc 'mkdir -p /root/.ssh && chmod 700 /root/.ssh && printf "%s\n" "{}" >> /root/.ssh/authorized_keys && chmod 600 /root/.ssh/authorized_keys'"#,
            key.replace('"', "\\\"")
        ),
    )
    .await?;
    Ok(())
}

fn join_nodes(nodes: &[String]) -> String {
    nodes.join(" ")
}

async fn stage_distributed_tests(vm: &mut Machine, repo_root: &Path) -> Result<()> {
    let dist_tests = repo_root.join("project/slayerfs/tests/scripts/distributed-tests");
    let target_dir = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| repo_root.join("project/target"));
    let dist_bin = target_dir.join("release/examples/persistence_demo");

    if !dist_bin.exists() {
        anyhow::bail!(
            "slayerfs example binary not found at {} (build it with `cargo build -p slayerfs --example persistence_demo --release`)",
            dist_bin.display()
        );
    }

    exec_check(vm, "mkdir -p /opt/slayerfs-dist-tests").await?;

    vm.upload(&dist_tests, Path::new("/opt/slayerfs-dist-tests"))
        .await
        .context("upload distributed-tests scripts")?;

    exec_check(
        vm,
        &format!("chmod +x {DIST_TESTS_DIR}/run-distributed-tests.sh"),
    )
    .await?;

    exec_check(vm, &format!("mkdir -p {DIST_WORKDIR}/bin")).await?;

    vm.upload(&dist_bin, Path::new(&format!("{DIST_WORKDIR}/bin")))
        .await
        .context("upload slayerfs persistence_demo binary")?;

    exec_check(vm, &format!("chmod +x {DIST_WORKDIR}/bin/persistence_demo")).await?;

    Ok(())
}

fn render_cluster_env(
    repo_root: &Path,
    client_nodes: &[String],
    meta_nodes: &[String],
    backend: MetaBackend,
    backend_urls: &[String],
) -> Result<PathBuf> {
    let tests_root = repo_root.join("project/slayerfs/tests/scripts/distributed-tests");
    let env_path = tests_root.join("cluster.env");

    let client_nodes_str = join_nodes(client_nodes);
    let meta_nodes_str = join_nodes(meta_nodes);
    let cluster_nodes_str = client_nodes_str.clone();
    let primary_client = client_nodes.first().cloned().unwrap_or_default();
    if primary_client.is_empty() {
        anyhow::bail!("client_nodes is empty; need at least one client node");
    }

    let mut env = format!(
        r#"# Auto-generated by qlean multinode test
CLUSTER_NODES="{cluster_nodes}"
CLIENT_NODES="{client_nodes}"
META_NODES="{meta_nodes}"
PRIMARY_CLIENT="{primary_client}"

SSH_USER="root"
SSH_KEY="$HOME/.ssh/id_ed25519"
SSH_PORT=22
SSH_OPTS="-o BatchMode=yes -o StrictHostKeyChecking=no"
REMOTE_SUDO="sudo -n"

REMOTE_WORKDIR="{remote_workdir}"
REMOTE_RESULTS_DIR="{remote_results}"
RESULTS_DIR="{results_dir}"

SLAYERFS_BUILD=0
SLAYERFS_EXAMPLE="persistence_demo"
SLAYERFS_BIN_LOCAL="{bin_local}"
SLAYERFS_META_BACKEND="{backend}"
SLAYERFS_MOUNT_DIR="/mnt/slayerfs"
SLAYERFS_META_DIR="/var/lib/slayerfs/meta"
SLAYERFS_DATA_DIR="/var/lib/slayerfs/data"
SLAYERFS_LOG_DIR="/var/log/slayerfs"
ETCD_DATA_DIR="/var/lib/etcd"
ETCD_LOG_FILE="/tmp/etcd.log"

RUN_FIO=1
RUN_MDTEST=0
RUN_IOZONE=0
RUN_XFSTESTS=0

# Light-weight test knobs
FIO_RUNTIME=5
FIO_SIZE=128M
FIO_NUMJOBS=1
FIO_IODEPTH=4
FIO_DIRECT=0
"#,
        cluster_nodes = cluster_nodes_str,
        client_nodes = client_nodes_str,
        meta_nodes = meta_nodes_str,
        primary_client = primary_client,
        remote_workdir = DIST_WORKDIR,
        remote_results = DIST_RESULTS_DIR,
        results_dir = tests_root.join("results").display(),
        backend = backend.as_str(),
        bin_local = format_args!("{}/bin/persistence_demo", DIST_WORKDIR),
    );

    match backend {
        MetaBackend::Etcd => {
            env.push_str(&format!(
                "SLAYERFS_META_ETCD_URLS=\"{}\"\n",
                backend_urls.join(",")
            ));
        }
        MetaBackend::Redis | MetaBackend::Postgres => {
            if let Some(url) = backend_urls.first() {
                env.push_str(&format!("SLAYERFS_META_URL=\"{}\"\n", url));
            }
        }
    }

    std::fs::write(&env_path, env)
        .with_context(|| format!("failed to write cluster env at {}", env_path.display()))?;

    Ok(env_path)
}

async fn run_distributed_tests(
    coordinator: &mut Machine,
    repo_root: &Path,
    client_nodes: &[String],
    meta_nodes: &[String],
    backend: MetaBackend,
    backend_urls: &[String],
) -> Result<()> {
    let env_path = render_cluster_env(repo_root, client_nodes, meta_nodes, backend, backend_urls)?;

    stage_distributed_tests(coordinator, repo_root)
        .await
        .context("stage distributed-tests assets on coordinator VM")?;

    coordinator
        .upload(&env_path, Path::new(DIST_TESTS_DIR))
        .await
        .context("upload cluster.env to coordinator VM")?;

    let timeout = std::time::Duration::from_secs(DIST_RUN_TIMEOUT_SECS);
    let cmd = format!("sh -lc 'cd {DIST_TESTS_DIR} && ./run-distributed-tests.sh all'",);
    if let Err(e) = exec_timed(coordinator, &cmd, timeout)
        .await
        .context("run distributed tests via qlean")
    {
        let diag = format!(
            r#"for node in {nodes}; do
  echo "=== slayerfs log ($node) ===";
  ssh -o BatchMode=yes -o StrictHostKeyChecking=no -i /root/.ssh/id_ed25519 root@$node \
    "tail -n 200 /var/log/slayerfs/slayerfs-$node.log 2>/dev/null || true" || true;
done"#,
            nodes = client_nodes.join(" ")
        );
        let diag_out = exec(coordinator, &format!("sh -lc '{}'", diag)).await?;
        return Err(e.context(diag_out));
    }

    let host_results_root =
        repo_root.join("project/slayerfs/tests/scripts/distributed-tests/results");
    coordinator
        .download(Path::new(DIST_RESULTS_DIR), &host_results_root)
        .await
        .context("download distributed test results")?;

    Ok(())
}

async fn cleanup_etcd_metadata_remote(vm: &mut Machine, endpoint: &str) -> Result<()> {
    let prefixes = [
        "slayerfs:",
        "f:",
        "r:",
        "p:",
        "l:",
        "session:",
        "session_info:",
        "slices/",
    ];
    for prefix in prefixes {
        let cmd =
            format!("sh -lc 'ETCDCTL_API=3 etcdctl --endpoints={endpoint} del --prefix {prefix}'",);
        exec_check(vm, &cmd).await?;
    }
    Ok(())
}

async fn cleanup_redis_metadata_remote(vm: &mut Machine, host: &str) -> Result<()> {
    let cmd = format!("sh -lc 'redis-cli -h {host} -p 6379 FLUSHALL'");
    exec_check(vm, &cmd).await?;
    Ok(())
}

async fn cleanup_postgres_metadata_remote(vm: &mut Machine, host: &str) -> Result<()> {
    let cmd = format!(
        "sh -lc 'PGPASSWORD=slayerfs psql -h {host} -p 15432 -U slayerfs -d database -c \"DROP SCHEMA public CASCADE; CREATE SCHEMA public;\"'",
    );
    exec_check(vm, &cmd).await?;
    Ok(())
}

async fn compute_vm_ip(vm: &mut Machine) -> Result<String> {
    match vm.get_ip().await {
        Ok(ip) if !ip.trim().is_empty() => return Ok(ip.trim().to_string()),
        Ok(_) => {}
        Err(e) => {
            tracing::info!(error = %e, "qlean::Machine::get_ip() failed; falling back to hostname -I");
        }
    }

    let out = exec_check(vm, "sh -lc \"hostname -I | awk '{print $1}'\"").await?;
    let ip = out.trim();
    if ip.is_empty() {
        anyhow::bail!("failed to compute VM IP: hostname -I returned empty output");
    }
    Ok(ip.to_string())
}

fn backend_urls_for(backend: MetaBackend, default_host: &str) -> Vec<String> {
    match backend {
        MetaBackend::Etcd => vec![format!("http://{default_host}:2379")],
        MetaBackend::Redis => vec![format!("redis://{default_host}:6379/0")],
        MetaBackend::Postgres => vec![format!(
            "postgres://slayerfs:slayerfs@{default_host}:15432/database"
        )],
    }
}

async fn default_gateway_ip(vm: &mut Machine) -> Result<Option<String>> {
    let out = exec_check(
        vm,
        r#"sh -lc "ip route | awk '/default/ {print \$3; exit}'""#,
    )
    .await?;
    let ip = out.trim();
    if ip.is_empty() {
        Ok(None)
    } else {
        Ok(Some(ip.to_string()))
    }
}

fn guess_gateway_from_ip(ip: &str) -> Option<String> {
    let parts: Vec<&str> = ip.trim().split('.').collect();
    if parts.len() != 4 || parts.iter().any(|p| p.is_empty()) {
        return None;
    }
    Some(format!("{}.{}.{}.1", parts[0], parts[1], parts[2]))
}

#[tokio::test]
#[ignore]
async fn test_slayerfs_qlean_multinode_smoke() -> Result<()> {
    tracing_subscriber_init();

    let backends = [MetaBackend::Etcd, MetaBackend::Redis, MetaBackend::Postgres];

    let run_id = host_run_id();
    let host_init_dir = PathBuf::from(HOST_DIAG_ROOT)
        .join("matrix")
        .join(&run_id)
        .join("host-init");
    tokio::fs::create_dir_all(&host_init_dir)
        .await
        .with_context(|| {
            format!(
                "failed to create host init diagnostics dir at {}",
                host_init_dir.display()
            )
        })?;
    tracing::info!(path = %host_init_dir.display(), run_id = %run_id, "prepared host-side diagnostics directory");

    let image = create_image(Distro::Debian, "debian-13-generic-amd64").await?;
    let config = MachineConfig {
        core: 2,
        mem: 2048,
        disk: Some(12),
        clear: true,
    };

    let topology = [ROLE_CLIENT1, ROLE_CLIENT2];
    maybe_run_single_vm_probe(&image, &config).await?;
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("slayerfs crate root")
        .parent()
        .expect("project root")
        .to_path_buf();

    with_pool(|pool: &mut MachinePool| {
        let image = image;
        Box::pin(async move {
            pool.add(ROLE_CLIENT1.to_string(), &image, &config)
                .await
                .with_context(|| format!("failed to add VM '{ROLE_CLIENT1}' to qlean pool"))?;
            pool.add(ROLE_CLIENT2.to_string(), &image, &config)
                .await
                .with_context(|| format!("failed to add VM '{ROLE_CLIENT2}' to qlean pool"))?;

            // Initialize VMs sequentially to avoid concurrent SSH timeouts
            // observed when probing multiple VMs in parallel (qlean 0.2.3 uses
            // a 60s SSH timeout when KVM is available). Sequential init reduces
            // contention and gives each VM time to boot and accept SSH.
            tracing::info!("init client1 -> init client2");
            tracing::info!(?topology, "defined multi-VM topology");
            for name in &topology {
                tracing::info!(vm = %name, "init: start");
                let mut vm = pool
                    .get(name)
                    .await
                    .with_context(|| format!("VM '{name}' missing from qlean pool"))?;
                if let Err(e) = vm
                    .init()
                    .await
                    .with_context(|| format!("failed to init VM '{name}'"))
                {
                    tracing::error!(vm = %name, host_init_dir = %host_init_dir.display(), "init: failed; collecting host-side diagnostics");
                    collect_host_init_failure(&host_init_dir, name, &e).await;
                    return Err(e);
                }
                tracing::info!(vm = %name, "init: done");
            }
            // Spawn all VMs after they've been initialized. spawn_all() is fast
            // and can run concurrently.
            pool.spawn_all()
                .await
                .context("qlean pool spawn_all() failed")?;

            let body_res: Result<()> = async {
                let mut client1 = pool
                    .get(ROLE_CLIENT1)
                    .await
                    .with_context(|| format!("VM '{ROLE_CLIENT1}' missing from qlean pool"))?;
                let mut client2 = pool
                    .get(ROLE_CLIENT2)
                    .await
                    .with_context(|| format!("VM '{ROLE_CLIENT2}' missing from qlean pool"))?;

                let client1_ip = compute_vm_ip(&mut client1).await?;
                let client2_ip = compute_vm_ip(&mut client2).await?;
                tracing::info!(%client1_ip, %client2_ip, "computed client VM IPs");

                let gateway_ip = default_gateway_ip(&mut client1)
                    .await
                    .context("client1: failed to detect default gateway")?;
                let fallback_ip = gateway_ip
                    .clone()
                    .or_else(|| guess_gateway_from_ip(&client1_ip))
                    .unwrap_or_else(|| "127.0.0.1".to_string());
                tracing::info!(gateway_ip = %fallback_ip, "selected backend host for default URLs");

                tracing::info!(
                    client_fuse_mount = CLIENT_FUSE_MOUNT,
                    "defined fixed directory topology"
                );

                ensure_base_packages(
                    &mut client1,
                    &[
                        "curl",
                        "jq",
                        "util-linux",
                        "procps",
                        "iproute2",
                        "fuse3",
                        "fio",
                    ],
                )
                .await
                .context("client1: failed to install base packages")?;
                ensure_base_packages(
                    &mut client2,
                    &[
                        "curl",
                        "jq",
                        "util-linux",
                        "procps",
                        "iproute2",
                        "fuse3",
                        "fio",
                    ],
                )
                .await
                .context("client2: failed to install base packages")?;

                ensure_vm_ssh(&mut client1)
                    .await
                    .context("client1: failed to ensure ssh")?;
                ensure_vm_ssh(&mut client2)
                    .await
                    .context("client2: failed to ensure ssh")?;
                configure_ssh_keypair(&mut client1)
                    .await
                    .context("client1: failed to configure ssh keypair")?;

                let client1_pub = read_vm_file(&mut client1, "/root/.ssh/id_ed25519.pub")
                    .await
                    .context("client1: failed to read public key")?;
                append_authorized_key(&mut client1, &client1_pub)
                    .await
                    .context("client1: failed to authorize self key")?;
                append_authorized_key(&mut client2, &client1_pub)
                    .await
                    .context("client2: failed to authorize client1 key")?;

                let client_nodes = vec![client1_ip.clone(), client2_ip.clone()];
                let meta_nodes = client_nodes.clone();

                for backend in backends {
                    if backend == MetaBackend::Etcd {
                        ensure_etcdctl(&mut client1)
                            .await
                            .context("client1: failed to install etcdctl")?;
                        cleanup_etcd_metadata_remote(&mut client1, &format!(
                            "http://{}:2379",
                            fallback_ip
                        ))
                        .await
                        .context("failed to cleanup etcd metadata")?;
                    }
                    if backend == MetaBackend::Redis {
                        ensure_redis_cli(&mut client1)
                            .await
                            .context("client1: failed to install redis-cli")?;
                        cleanup_redis_metadata_remote(&mut client1, &fallback_ip)
                            .await
                            .context("failed to cleanup redis metadata")?;
                    }
                    if backend == MetaBackend::Postgres {
                        ensure_psql(&mut client1)
                            .await
                            .context("client1: failed to install psql")?;
                        cleanup_postgres_metadata_remote(&mut client1, &fallback_ip)
                            .await
                            .context("failed to cleanup postgres metadata")?;
                    }
                    let backend_urls = backend_urls_for(backend, &fallback_ip);
                    tracing::info!(backend = backend.as_str(), urls = ?backend_urls, "starting backend test run");

                    let run_res = run_distributed_tests(
                        &mut client1,
                        &repo_root,
                        &client_nodes,
                        &meta_nodes,
                        backend,
                        &backend_urls,
                    )
                    .await
                    .with_context(|| format!("distributed tests failed for backend {}", backend.as_str()));

                    let _ = exec(
                        &mut client1,
                        &format!(
                            "sh -lc 'cd {DIST_TESTS_DIR} && ./run-distributed-tests.sh cleanup'",
                        ),
                    )
                    .await;

                    run_res?;
                }

                Ok(())
            }
            .await;

            pool.shutdown_all()
                .await
                .context("qlean pool shutdown_all() failed")?;

            cleanup_qlean_runs().await;
            body_res
        })
    })
    .await?;

    Ok(())
}
