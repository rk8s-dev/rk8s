//! Container health probe implementations.
//!
//! This module defines the [`Prober`] trait and three concrete implementations for container
//! health checking. Supported probe types are:
//! - Exec: runs a command inside the container
//! - HTTP GET: sends an HTTP request to the container
//! - TCP socket: attempts a TCP connection to the container
//!
//! Each prober carries a [`ProbeConfig`] that controls timing (initial delay, period, timeout)
//! and thresholds (success and failure counts).

use std::{any::Any, path::Path, time::Duration};

use libcontainer::syscall::syscall::create_syscall;
use libruntime::rootpath;
use tokio::{
    io::{AsyncBufReadExt as _, AsyncReadExt as _, AsyncWriteExt as _},
    net::TcpStream,
};
use uuid::Uuid;

use crate::commands::{Exec, exec, pod::PodInfo};

/// Interface for container health probes.
///
/// Implementers define the probe mechanism (exec, HTTP, or TCP) and execute the probe
/// when `probe()` is called.
#[async_trait::async_trait]
#[allow(unused)]
pub trait Prober: Any {
    /// Executes the probe.
    ///
    /// Returns `Ok(())` on success, `Err` on probe failure.
    /// Timeout behavior is applied by the caller.
    async fn probe(&self) -> anyhow::Result<()>;

    /// Returns the kind of probe (Exec, HttpGet, or TcpSocket).
    fn probe_kind(&self) -> ProbeKind;

    /// Returns the probe's configuration.
    fn config(&self) -> &ProbeConfig;

    /// Downcasts to the concrete type for testing or inspection.
    fn as_any(&self) -> &dyn Any;
}

/// Configuration shared by all probe types.
#[derive(Clone, Debug)]
pub struct ProbeConfig {
    /// UID of the pod being probed.
    pub pod_id: Uuid,
    /// Name of the pod.
    pub pod_name: String,
    /// Name of the container within the pod.
    pub container_name: String,
    /// Time to wait after container start before first probe.
    pub initial_delay: Duration,
    /// Interval between consecutive probes.
    pub period: Duration,
    /// Maximum time to wait for a single probe to complete.
    pub timeout: Duration,
    /// Consecutive successes required to mark the probe as passing.
    pub success_threshold: u32,
    /// Consecutive failures required to mark the probe as failing.
    pub failure_threshold: u32,
}

impl Default for ProbeConfig {
    fn default() -> Self {
        Self {
            pod_id: Uuid::nil(),
            pod_name: String::new(),
            container_name: String::new(),
            initial_delay: Duration::from_secs(0),
            period: Duration::from_secs(10),
            timeout: Duration::from_secs(1),
            success_threshold: 1,
            failure_threshold: 3,
        }
    }
}

/// Discriminant for the probe mechanism.
#[derive(Clone, Debug)]
#[allow(unused)]
pub enum ProbeKind {
    /// Executes a command inside the container.
    Exec,
    /// Sends an HTTP GET request to the container.
    HttpGet,
    /// Attempts a TCP connection to the container.
    TcpSocket,
}

/// Runs a command inside the container via the runtime exec API.
pub struct ExecProber {
    command: Vec<String>,
    config: ProbeConfig,
}

/// Sends an HTTP GET request to the container.
pub struct HttpGetProber {
    host: String,
    port: u16,
    path: String,
    config: ProbeConfig,
}

/// Attempts a TCP connection to the container.
pub struct TcpSocketProber {
    host: String,
    port: u16,
    config: ProbeConfig,
}

impl ExecProber {
    /// Creates a new exec prober with the given command and config.
    pub fn new(command: Vec<String>, config: ProbeConfig) -> Self {
        Self { command, config }
    }
}

pub(crate) fn resolve_container_id(
    root_path: &Path,
    config: &ProbeConfig,
) -> anyhow::Result<String> {
    if !config.pod_id.is_nil() {
        let pod_info = PodInfo::load(root_path, &config.pod_name)?;
        if let Some(container_id) =
            match_container_name(&config.container_name, &pod_info.container_names)
        {
            return Ok(container_id);
        }

        return Err(anyhow::anyhow!(
            "container {} not found in pod {} (uid={})",
            config.container_name,
            config.pod_name,
            config.pod_id
        ));
    }

    let direct_path = root_path.join(&config.container_name);
    if direct_path.exists() {
        return Ok(config.container_name.clone());
    }

    Err(anyhow::anyhow!(
        "container {} not found and pod {} could not be resolved",
        config.container_name,
        config.pod_id
    ))
}

pub(crate) fn match_container_name(name: &str, candidates: &[String]) -> Option<String> {
    if candidates.iter().any(|candidate| candidate == name) {
        return Some(name.to_string());
    }

    let suffix = format!("-{name}");
    candidates
        .iter()
        .find(|candidate| candidate.ends_with(&suffix))
        .cloned()
}

#[async_trait::async_trait]
impl Prober for ExecProber {
    async fn probe(&self) -> anyhow::Result<()> {
        if self.command.is_empty() {
            return Err(anyhow::anyhow!("exec probe command cannot be empty"));
        }

        let root_path = rootpath::determine(None, &*create_syscall())?;
        let container_id = resolve_container_id(&root_path, &self.config)?;
        let command = self.command.clone();

        let exec_args = Exec {
            pod_name: Some(self.config.pod_name.clone()),
            container_id: container_id.clone(),
            command,
            console_socket: None,
            cwd: None,
            env: Vec::new(),
            tty: false,
            user: None,
            additional_gids: Vec::new(),
            process: None,
            detach: false,
            pid_file: None,
            process_label: None,
            apparmor: None,
            no_new_privs: false,
            cap: Vec::new(),
            preserve_fds: 0,
            ignore_paused: false,
            cgroup: None,
        };

        // exec() calls waitpid() which is a blocking syscall. Run it on a
        // dedicated blocking thread so we don't stall the tokio runtime and
        // so that tokio::time::timeout can actually cancel the future.
        let exit_code = tokio::task::spawn_blocking(move || exec(exec_args, root_path))
            .await
            .map_err(|e| anyhow::anyhow!("exec probe join error: {e}"))?
            .map_err(|e| anyhow::anyhow!("exec probe error: {e}"))?;

        if exit_code == 0 {
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "exec probe failed for container {container_id} with status {exit_code}"
            ))
        }
    }

    fn probe_kind(&self) -> ProbeKind {
        ProbeKind::Exec
    }

    fn config(&self) -> &ProbeConfig {
        &self.config
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[allow(unused)]
impl HttpGetProber {
    /// Creates a new HTTP GET prober.
    pub fn new(host: String, port: u16, path: String, config: ProbeConfig) -> Self {
        Self {
            host,
            port,
            path,
            config,
        }
    }

    /// Returns the target host.
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Returns the target port.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Returns the request path.
    pub fn path(&self) -> &str {
        &self.path
    }
}

#[async_trait::async_trait]
impl Prober for HttpGetProber {
    async fn probe(&self) -> anyhow::Result<()> {
        let addr = format!("{}:{}", self.host, self.port);
        let mut stream = TcpStream::connect(&addr).await?;

        let request = format!(
            "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: rkl-probe/0.1\r\nConnection: close\r\n\r\n",
            self.path, self.host
        );

        stream.write_all(request.as_bytes()).await?;

        // Use BufReader to reliably read the complete status line, even if the
        // response arrives across multiple TCP segments.
        let reader = tokio::io::BufReader::new(&mut stream);
        let mut status_line = String::new();
        const MAX_STATUS_LINE_LEN: usize = 512;
        reader
            .take(MAX_STATUS_LINE_LEN as u64)
            .read_line(&mut status_line)
            .await?;

        if status_line.is_empty() {
            return Err(anyhow::anyhow!("http probe received empty response"));
        }

        let mut parts = status_line.split_whitespace();
        let _http_version = parts.next();
        let status_code_str = parts.next().ok_or_else(|| {
            anyhow::anyhow!("http probe received malformed status line: {status_line}")
        })?;
        let status_code: u16 = status_code_str.parse().map_err(|e| {
            anyhow::anyhow!(
                "http probe could not parse status code '{status_code_str}' in '{status_line}': {e}"
            )
        })?;

        if (200..400).contains(&status_code) {
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "http probe non-success status: {status_line}"
            ))
        }
    }

    fn probe_kind(&self) -> ProbeKind {
        ProbeKind::HttpGet
    }

    fn config(&self) -> &ProbeConfig {
        &self.config
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[allow(unused)]
impl TcpSocketProber {
    /// Creates a new TCP socket prober.
    pub fn new(host: String, port: u16, config: ProbeConfig) -> Self {
        Self { host, port, config }
    }

    /// Returns the target host.
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Returns the target port.
    pub fn port(&self) -> u16 {
        self.port
    }
}

#[async_trait::async_trait]
impl Prober for TcpSocketProber {
    async fn probe(&self) -> anyhow::Result<()> {
        let addr = format!("{}:{}", self.host, self.port);
        let mut stream = TcpStream::connect(&addr).await?;
        stream.shutdown().await?;

        Ok(())
    }

    fn probe_kind(&self) -> ProbeKind {
        ProbeKind::TcpSocket
    }

    fn config(&self) -> &ProbeConfig {
        &self.config
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;
    use uuid::Uuid;

    fn write_pod_info(root: &Path, pod_name: &str, containers: &[&str]) {
        let pods_dir = root.join("pods");
        fs::create_dir_all(&pods_dir).expect("create pods dir");

        let mut contents = String::from("PodSandbox ID: sandbox\n");
        for name in containers {
            contents.push_str(&format!("- {name}\n"));
        }
        fs::write(pods_dir.join(pod_name), contents).expect("write pod info");
    }

    #[test]
    fn match_container_name_exact_match() {
        let candidates = vec!["app".to_string(), "sidecar".to_string()];
        let matched = match_container_name("app", &candidates);
        assert_eq!(matched, Some("app".to_string()));
    }

    #[test]
    fn match_container_name_suffix_match() {
        let candidates = vec!["pod-app".to_string(), "pod-sidecar".to_string()];
        let matched = match_container_name("app", &candidates);
        assert_eq!(matched, Some("pod-app".to_string()));
    }

    #[test]
    fn resolve_container_id_from_pod_info() {
        let dir = tempdir().expect("tempdir");
        write_pod_info(dir.path(), "demo", &["pod-app", "pod-sidecar"]);

        let config = ProbeConfig {
            pod_id: Uuid::new_v4(),
            pod_name: "demo".to_string(),
            container_name: "app".to_string(),
            ..Default::default()
        };
        let container_id = resolve_container_id(dir.path(), &config).expect("resolve");
        assert_eq!(container_id, "pod-app");
    }

    #[test]
    fn resolve_container_id_from_direct_path() {
        let dir = tempdir().expect("tempdir");
        let direct = dir.path().join("container-123");
        fs::create_dir_all(&direct).expect("create container dir");

        let config = ProbeConfig {
            pod_id: Uuid::nil(),
            pod_name: "ignored".to_string(),
            container_name: "container-123".to_string(),
            ..Default::default()
        };
        let container_id = resolve_container_id(dir.path(), &config).expect("resolve");
        assert_eq!(container_id, "container-123");
    }

    #[test]
    fn resolve_container_id_errors_when_missing() {
        let dir = tempdir().expect("tempdir");
        let config = ProbeConfig {
            pod_id: Uuid::nil(),
            pod_name: "missing".to_string(),
            container_name: "nope".to_string(),
            ..Default::default()
        };
        assert!(resolve_container_id(dir.path(), &config).is_err());
    }

    #[test]
    fn resolve_container_id_errors_when_pod_info_missing() {
        let dir = tempdir().expect("tempdir");
        let config = ProbeConfig {
            pod_id: Uuid::new_v4(),
            pod_name: "missing".to_string(),
            container_name: "app".to_string(),
            ..Default::default()
        };
        assert!(resolve_container_id(dir.path(), &config).is_err());
    }

    #[test]
    fn resolve_container_id_errors_when_container_not_in_pod_info() {
        let dir = tempdir().expect("tempdir");
        write_pod_info(dir.path(), "demo", &["pod-sidecar"]);

        let config = ProbeConfig {
            pod_id: Uuid::new_v4(),
            pod_name: "demo".to_string(),
            container_name: "app".to_string(),
            ..Default::default()
        };
        assert!(resolve_container_id(dir.path(), &config).is_err());
    }

    #[tokio::test]
    async fn exec_prober_empty_command_errors() {
        let prober = ExecProber::new(Vec::new(), ProbeConfig::default());
        let result = prober.probe().await;
        assert!(result.is_err());
    }

    #[test]
    fn exec_prober_stores_command() {
        let command = vec!["/bin/true".to_string()];
        let prober = ExecProber::new(command.clone(), ProbeConfig::default());
        assert_eq!(prober.command, command);
        assert!(matches!(prober.probe_kind(), ProbeKind::Exec));
    }

    #[test]
    fn http_get_prober_stores_fields() {
        let prober = HttpGetProber::new(
            "127.0.0.1".to_string(),
            8080,
            "/health".to_string(),
            ProbeConfig::default(),
        );
        assert_eq!(prober.host, "127.0.0.1");
        assert_eq!(prober.port, 8080);
        assert_eq!(prober.path, "/health");
        assert!(matches!(prober.probe_kind(), ProbeKind::HttpGet));
    }

    #[test]
    fn tcp_socket_prober_stores_fields() {
        let prober = TcpSocketProber::new("127.0.0.1".to_string(), 9090, ProbeConfig::default());
        assert_eq!(prober.host, "127.0.0.1");
        assert_eq!(prober.port, 9090);
        assert!(matches!(prober.probe_kind(), ProbeKind::TcpSocket));
    }
}
