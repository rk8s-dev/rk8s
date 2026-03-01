use common::ContainerSpec;
use libcontainer::container::{Container, ContainerStatus};
use liboci_cli::{Delete, Kill};
use rkforge::commands::{delete, kill};
use rkforge::commands::container::ContainerRunner;
use serial_test::serial;
use std::os::unix::fs::symlink;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use uuid::Uuid;

mod test_common;
use test_common::bundles_path;

struct Cleanup {
    root_path: PathBuf,
    container_id: String,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = kill(
            Kill {
                container_id: self.container_id.clone(),
                signal: "SIGKILL".to_string(),
                all: false,
            },
            self.root_path.clone(),
        );
        let _ = delete(
            Delete {
                container_id: self.container_id.clone(),
                force: true,
            },
            self.root_path.clone(),
        );
        let _ = std::fs::remove_dir_all(self.root_path.join(&self.container_id));
    }
}

fn enabled() -> bool {
    std::env::var("RKFORGE_LIFECYCLE_TESTS")
        .map(|v| v == "1")
        .unwrap_or(false)
}

fn load_status(root_path: &PathBuf, id: &str) -> ContainerStatus {
    let container = Container::load(root_path.join(id)).unwrap();
    container.status()
}

fn wait_for_status(root_path: &PathBuf, id: &str, expected: ContainerStatus) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let status = load_status(root_path, id);
        if status == expected {
            return;
        }
        if Instant::now() >= deadline {
            panic!("container {id} status {status:?}, expected {expected:?}");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn proc_starttime(pid: i32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let end = stat.rfind(')')?;
    let after = stat.get(end + 2..)?;
    let mut it = after.split_whitespace();
    for _ in 0..19 {
        it.next()?;
    }
    it.next()?.parse::<u64>().ok()
}

fn proc_state(pid: i32) -> Option<char> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let end = stat.rfind(')')?;
    let after = stat.get(end + 2..)?;
    after.chars().next()
}

fn try_reap(pid: i32) -> bool {
    match nix::sys::wait::waitpid(
        nix::unistd::Pid::from_raw(pid),
        Some(nix::sys::wait::WaitPidFlag::WNOHANG),
    ) {
        Ok(nix::sys::wait::WaitStatus::StillAlive) => false,
        Ok(_) => true,
        Err(nix::errno::Errno::ECHILD) => false,
        Err(_) => false,
    }
}

fn wait_for_terminated(pid: i32, original_starttime: Option<u64>) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let starttime = proc_starttime(pid);
        let same_process = match (original_starttime, starttime) {
            (_, None) => return,
            (Some(orig), Some(now)) => now == orig,
            (None, Some(_)) => true,
        };
        if !same_process {
            return;
        }

        if proc_state(pid) == Some('Z') {
            let _ = try_reap(pid);
            return;
        }

        if Instant::now() >= deadline {
            panic!("container init pid {pid} did not terminate");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn stop_container(root_path: &PathBuf, id: &str, pid: i32, original_starttime: Option<u64>) {
    let _ = kill(
        Kill {
            container_id: id.to_string(),
            signal: "SIGTERM".to_string(),
            all: false,
        },
        root_path.clone(),
    );

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let starttime = proc_starttime(pid);
        let exited = match (original_starttime, starttime) {
            (_, None) => true,
            (Some(orig), Some(now)) => now != orig,
            (None, Some(_)) => false,
        };
        if exited {
            break;
        }
        if Instant::now() >= deadline {
            let _ = kill(
                Kill {
                    container_id: id.to_string(),
                    signal: "SIGKILL".to_string(),
                    all: false,
                },
                root_path.clone(),
            );
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    wait_for_status(root_path, id, ContainerStatus::Stopped);
    wait_for_terminated(pid, original_starttime);
}

fn make_spec(id: &str, bundle_path: String) -> ContainerSpec {
    ContainerSpec {
        name: id.to_string(),
        image: bundle_path,
        args: vec!["sleep".to_string(), "1000".to_string()],
        ports: vec![],
        resources: None,
        liveness_probe: None,
        readiness_probe: None,
        startup_probe: None,
        security_context: None,
        env: None,
        volume_mounts: None,
        command: None,
        working_dir: None,
    }
}

fn prepare_bundle(base_bundle: &str, root_dir: &tempfile::TempDir) -> String {
    let base = PathBuf::from(base_bundle);
    let bundle_dir = root_dir.path().join("bundle");
    std::fs::create_dir_all(&bundle_dir).unwrap();

    let rootfs = base.join("rootfs");
    if rootfs.exists() {
        symlink(rootfs, bundle_dir.join("rootfs")).unwrap();
    }

    let config_json = base.join("config.json");
    if config_json.exists() {
        std::fs::copy(config_json, bundle_dir.join("config.json")).unwrap();
    }

    bundle_dir.to_string_lossy().to_string()
}

#[test]
#[serial]
fn test_container_lifecycle_create_start_stop_rm() {
    if !enabled() {
        return;
    }

    let root_dir = tempfile::tempdir().unwrap();
    let root_path = root_dir.path().join("youki");
    std::fs::create_dir_all(&root_path).unwrap();

    let bundle_path = prepare_bundle(&bundles_path("busybox"), &root_dir);
    let id = format!("rkforge-lifecycle-{}", Uuid::new_v4());
    let _cleanup = Cleanup {
        root_path: root_path.clone(),
        container_id: id.clone(),
    };

    let mut runner =
        ContainerRunner::from_spec(make_spec(&id, bundle_path), Some(root_path.clone())).unwrap();
    runner.create().unwrap();
    wait_for_status(&root_path, &id, ContainerStatus::Created);

    runner.start_container(Some(id.clone())).unwrap();
    wait_for_status(&root_path, &id, ContainerStatus::Running);
    let pid = Container::load(root_path.join(&id))
        .unwrap()
        .pid()
        .unwrap()
        .as_raw();
    let starttime = proc_starttime(pid);
    stop_container(&root_path, &id, pid, starttime);

    delete(
        Delete {
            container_id: id.clone(),
            force: true,
        },
        root_path.clone(),
    )
    .unwrap();

    assert!(!root_path.join(&id).exists());
}

#[test]
#[serial]
fn test_container_lifecycle_run_kill_rm() {
    if !enabled() {
        return;
    }

    let root_dir = tempfile::tempdir().unwrap();
    let root_path = root_dir.path().join("youki");
    std::fs::create_dir_all(&root_path).unwrap();

    let bundle_path = prepare_bundle(&bundles_path("busybox"), &root_dir);
    let id = format!("rkforge-lifecycle-{}", Uuid::new_v4());
    let _cleanup = Cleanup {
        root_path: root_path.clone(),
        container_id: id.clone(),
    };

    let mut runner =
        ContainerRunner::from_spec(make_spec(&id, bundle_path), Some(root_path.clone())).unwrap();
    runner.run().unwrap();
    wait_for_status(&root_path, &id, ContainerStatus::Running);
    let pid = Container::load(root_path.join(&id))
        .unwrap()
        .pid()
        .unwrap()
        .as_raw();
    let starttime = proc_starttime(pid);

    kill(
        Kill {
            container_id: id.clone(),
            signal: "SIGKILL".to_string(),
            all: false,
        },
        root_path.clone(),
    )
    .unwrap();
    wait_for_status(&root_path, &id, ContainerStatus::Stopped);
    wait_for_terminated(pid, starttime);

    delete(
        Delete {
            container_id: id.clone(),
            force: true,
        },
        root_path.clone(),
    )
    .unwrap();

    assert!(!root_path.join(&id).exists());
}
