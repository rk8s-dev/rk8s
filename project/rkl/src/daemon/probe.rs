use std::{
    collections::HashMap,
    fmt,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};

use anyhow::{Context, Result, anyhow};
use lazy_static::lazy_static;
use tracing::{debug, warn};

use common::{ContainerProbeStatus, ContainerStatus, PodTask, Probe, ProbeAction, ProbeCondition};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    process::Command,
    sync::{oneshot, watch},
    task::JoinHandle,
    time::{Interval, MissedTickBehavior, sleep, timeout},
};

lazy_static! {
    static ref PROBE_REGISTRY: Mutex<HashMap<String, PodProbes>> = Mutex::new(HashMap::new());
}

/// Configuration shared by all probe types.
#[derive(Clone, Debug)]
pub struct ProbeConfig {
    pub initial_delay: Duration,
    pub period: Duration,
    pub timeout: Duration,
    pub success_threshold: u32,
    pub failure_threshold: u32,
}

impl Default for ProbeConfig {
    fn default() -> Self {
        Self {
            initial_delay: Duration::from_secs(0),
            period: Duration::from_secs(10),
            timeout: Duration::from_secs(1),
            success_threshold: 1,
            failure_threshold: 3,
        }
    }
}

/// Supported probe execution strategies.
#[derive(Clone, Debug)]
pub enum ProbeKind {
    Exec {
        command: Vec<String>,
    },
    HttpGet {
        host: String,
        port: u16,
        path: String,
    },
    TcpSocket {
        host: String,
        port: u16,
    },
}

/// Complete specification of a probe.
#[derive(Clone, Debug)]
pub struct ProbeSpec {
    pub name: String,
    pub kind: ProbeKind,
    pub config: ProbeConfig,
}

impl ProbeSpec {
    pub fn new(name: impl Into<String>, kind: ProbeKind) -> Self {
        Self {
            name: name.into(),
            kind,
            config: ProbeConfig::default(),
        }
    }

    pub fn with_config(mut self, config: ProbeConfig) -> Self {
        self.config = config;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProbeClass {
    Readiness,
    Liveness,
    Startup,
}

impl ProbeClass {
    fn as_str(&self) -> &'static str {
        match self {
            ProbeClass::Readiness => "readiness",
            ProbeClass::Liveness => "liveness",
            ProbeClass::Startup => "startup",
        }
    }
}

pub struct ProbeRegistration {
    pub container: String,
    pub class: ProbeClass,
    pub spec: ProbeSpec,
}

struct ManagedProbe {
    container: String,
    class: ProbeClass,
    handle: ProbeHandle,
    status_rx: watch::Receiver<ProbeStatus>,
}

impl ManagedProbe {
    async fn shutdown(self) {
        self.handle.shutdown().await;
    }

    fn snapshot(&self) -> ProbeSnapshot {
        ProbeSnapshot {
            container: self.container.clone(),
            class: self.class,
            status: self.status_rx.borrow().clone(),
        }
    }
}

struct PodProbes {
    probes: Vec<ManagedProbe>,
}

impl PodProbes {
    fn snapshots(&self) -> Vec<ProbeSnapshot> {
        self.probes.iter().map(ManagedProbe::snapshot).collect()
    }

    async fn shutdown(self) {
        for probe in self.probes {
            probe.shutdown().await;
        }
    }
}

pub struct ProbeSnapshot {
    pub container: String,
    pub class: ProbeClass,
    pub status: ProbeStatus,
}

/// High-level state shared with observers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProbeState {
    Pending,
    Ready,
    Failing,
}

impl From<ProbeState> for ProbeCondition {
    fn from(value: ProbeState) -> Self {
        match value {
            ProbeState::Pending => ProbeCondition::Pending,
            ProbeState::Ready => ProbeCondition::Ready,
            ProbeState::Failing => ProbeCondition::Failing,
        }
    }
}

impl fmt::Display for ProbeState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProbeState::Pending => write!(f, "Pending"),
            ProbeState::Ready => write!(f, "Ready"),
            ProbeState::Failing => write!(f, "Failing"),
        }
    }
}

/// Fine-grained status snapshot.
#[derive(Clone, Debug)]
pub struct ProbeStatus {
    #[allow(dead_code)]
    pub name: String,
    pub state: ProbeState,
    pub consecutive_successes: u32,
    pub consecutive_failures: u32,
    pub last_error: Option<String>,
    pub last_transition_time: SystemTime,
}

fn to_container_probe_status(status: &ProbeStatus) -> ContainerProbeStatus {
    ContainerProbeStatus {
        state: status.state.into(),
        consecutive_successes: status.consecutive_successes,
        consecutive_failures: status.consecutive_failures,
        last_error: status.last_error.clone(),
    }
}

impl ProbeStatus {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            state: ProbeState::Pending,
            consecutive_successes: 0,
            consecutive_failures: 0,
            last_error: None,
            last_transition_time: SystemTime::now(),
        }
    }

    fn update_state(&mut self, state: ProbeState) {
        if self.state != state {
            self.state = state;
            self.last_transition_time = SystemTime::now();
        }
    }

    fn set_error(&mut self, err: anyhow::Error) {
        self.last_error = Some(err.to_string());
    }

    fn clear_error(&mut self) {
        self.last_error = None;
    }
}

/// Handle returned to callers for lifecycle management.
pub struct ProbeHandle {
    stop: Option<oneshot::Sender<()>>,
    join: JoinHandle<()>,
    status_rx: watch::Receiver<ProbeStatus>,
}

impl ProbeHandle {
    pub fn status(&self) -> watch::Receiver<ProbeStatus> {
        self.status_rx.clone()
    }

    pub async fn shutdown(mut self) {
        if let Some(tx) = self.stop.take() {
            let _ = tx.send(());
        }

        let _ = self.join.await;
    }
}

/// Spawn an async task to evaluate the provided probe specification.
pub fn spawn_probe(spec: ProbeSpec) -> ProbeHandle {
    let spec = Arc::new(spec);
    let (stop_tx, stop_rx) = oneshot::channel();
    let (status_tx, status_rx) = watch::channel(ProbeStatus::new(&spec.name));

    let join = tokio::spawn(run_probe_loop(spec, status_tx, stop_rx));

    ProbeHandle {
        stop: Some(stop_tx),
        join,
        status_rx,
    }
}

async fn run_probe_loop(
    spec: Arc<ProbeSpec>,
    status_tx: watch::Sender<ProbeStatus>,
    mut stop_rx: oneshot::Receiver<()>,
) {
    if spec.config.initial_delay.as_secs_f64() > 0.0 {
        tokio::select! {
            _ = sleep(spec.config.initial_delay) => {},
            _ = &mut stop_rx => return,
        }
    }

    let mut status = ProbeStatus::new(&spec.name);
    let mut interval = new_interval(spec.config.period);
    let mut failures = 0u32;
    let mut successes = 0u32;

    loop {
        tokio::select! {
            _ = &mut stop_rx => break,
            _ = interval.tick() => {
                match timeout(spec.config.timeout, exec_probe(&spec.kind)).await {
                    Ok(Ok(())) => {
                        failures = 0;
                        successes = successes.saturating_add(1);
                        status.clear_error();
                        status.consecutive_failures = 0;
                        status.consecutive_successes = successes;

                        if successes >= spec.config.success_threshold {
                            status.update_state(ProbeState::Ready);
                        }
                    }
                    Ok(Err(err)) => {
                        failures = failures.saturating_add(1);
                        successes = 0;
                        status.set_error(err);
                        status.consecutive_successes = 0;
                        status.consecutive_failures = failures;

                        if failures >= spec.config.failure_threshold {
                            status.update_state(ProbeState::Failing);
                        }
                    }
                    Err(_) => {
                        failures = failures.saturating_add(1);
                        successes = 0;
                        status.set_error(anyhow!("probe timed out after {:?}", spec.config.timeout));
                        status.consecutive_successes = 0;
                        status.consecutive_failures = failures;

                        if failures >= spec.config.failure_threshold {
                            status.update_state(ProbeState::Failing);
                        }
                    }
                }

                let _ = status_tx.send(status.clone());
            }
        }
    }
}

/// Returns a tokio interval for the given period, enforcing a minimum of 1 second.  
fn new_interval(period: Duration) -> Interval {
    let mut interval = tokio::time::interval(period);
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    interval
}

async fn exec_probe(kind: &ProbeKind) -> Result<()> {
    match kind {
        ProbeKind::Exec { command } => run_exec_probe(command).await,
        ProbeKind::HttpGet { host, port, path } => run_http_probe(host, *port, path).await,
        ProbeKind::TcpSocket { host, port } => run_tcp_probe(host, *port).await,
    }
}

async fn run_exec_probe(command: &[String]) -> Result<()> {
    let (program, args) = command
        .split_first()
        .ok_or_else(|| anyhow!("exec probe command cannot be empty"))?;

    let mut cmd = Command::new(program);
    cmd.args(args);

    let status = cmd.status().await?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("exec probe exited with status {status}"))
    }
}

async fn run_tcp_probe(host: &str, port: u16) -> Result<()> {
    let addr = format!("{host}:{port}");
    let mut stream = TcpStream::connect(&addr).await?;
    stream.shutdown().await?;
    Ok(())
}

async fn run_http_probe(host: &str, port: u16, path: &str) -> Result<()> {
    let addr = format!("{host}:{port}");
    let mut stream = TcpStream::connect(&addr).await?;

    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: rkl-probe/0.1\r\nConnection: close\r\n\r\n"
    );

    stream.write_all(request.as_bytes()).await?;

    // Use a conservative buffer size that accommodates typical HTTP status lines
    // and essential headers. Probe responses should be minimal - we only need
    // the status line to determine health.
    const HTTP_PROBE_BUFFER_SIZE: usize = 512;

    let mut buf = vec![0u8; HTTP_PROBE_BUFFER_SIZE];
    let n = stream.read(&mut buf).await?;
    if n == 0 {
        return Err(anyhow!("http probe received empty response"));
    }

    let response = std::str::from_utf8(&buf[..n])?;
    if response.starts_with("HTTP/1.1 200") || response.starts_with("HTTP/1.0 200") {
        Ok(())
    } else {
        let line = response.lines().next().unwrap_or("");
        Err(anyhow!("http probe non-success status: {line}"))
    }
}

fn derive_probe_config(base: &Probe, defaults: ProbeConfig) -> ProbeConfig {
    let mut config = defaults;
    if let Some(delay) = base.initial_delay_seconds {
        config.initial_delay = Duration::from_secs(delay as u64);
    }
    if let Some(period) = base.period_seconds
        && period > 0
    {
        config.period = Duration::from_secs(period as u64);
    }
    if let Some(timeout) = base.timeout_seconds
        && timeout > 0
    {
        config.timeout = Duration::from_secs(timeout as u64);
    }
    if let Some(success) = base.success_threshold {
        config.success_threshold = success.max(1);
    }
    if let Some(failure) = base.failure_threshold {
        config.failure_threshold = failure.max(1);
    }
    config
}

fn build_probe_spec(
    pod: &PodTask,
    container: &str,
    class: ProbeClass,
    probe: &Probe,
    pod_ip: &str,
) -> Result<Option<ProbeSpec>> {
    let config = derive_probe_config(probe, ProbeConfig::default());

    let Some(action) = &probe.action else {
        warn!(
            container = container,
            pod = %pod.metadata.name,
            "ignored probe with no action (exec/http/tcp)"
        );
        return Ok(None);
    };

    let kind = match action {
        ProbeAction::Exec(exec) => {
            if exec.command.is_empty() {
                warn!(
                    container = container,
                    pod = %pod.metadata.name,
                    "ignored exec probe with empty command"
                );
                return Ok(None);
            }
            ProbeKind::Exec {
                command: exec.command.clone(),
            }
        }
        ProbeAction::HttpGet(http) => {
            if http.port == 0 {
                warn!(
                    container = container,
                    pod = %pod.metadata.name,
                    "ignored http probe with port 0"
                );
                return Ok(None);
            }

            let host = http
                .host
                .clone()
                .or_else(|| (!pod_ip.is_empty()).then(|| pod_ip.to_string()))
                .ok_or_else(|| {
                    anyhow!(
                        "http probe requires host or pod IP (container={}, pod={})",
                        container,
                        pod.metadata.name
                    )
                })?;

            let path = if http.path.is_empty() {
                "/".to_string()
            } else {
                http.path.clone()
            };

            ProbeKind::HttpGet {
                host,
                port: http.port,
                path,
            }
        }
        ProbeAction::TcpSocket(tcp) => {
            if tcp.port == 0 {
                warn!(
                    container = container,
                    pod = %pod.metadata.name,
                    "ignored tcp probe with port 0"
                );
                return Ok(None);
            }

            let host = tcp
                .host
                .clone()
                .or_else(|| (!pod_ip.is_empty()).then(|| pod_ip.to_string()))
                .ok_or_else(|| {
                    anyhow!(
                        "tcp probe requires host or pod IP (container={}, pod={})",
                        container,
                        pod.metadata.name
                    )
                })?;

            ProbeKind::TcpSocket {
                host,
                port: tcp.port,
            }
        }
    };

    let spec = ProbeSpec::new(
        format!("{}/{}/{}", pod.metadata.name, container, class.as_str()),
        kind,
    )
    .with_config(config);
    Ok(Some(spec))
}

pub fn build_probe_registrations(pod: &PodTask, pod_ip: &str) -> Result<Vec<ProbeRegistration>> {
    let mut registrations = Vec::new();
    for container in &pod.spec.containers {
        if let Some(probe) = &container.readiness_probe
            && let Some(spec) =
                build_probe_spec(pod, &container.name, ProbeClass::Readiness, probe, pod_ip)?
        {
            registrations.push(ProbeRegistration {
                container: container.name.clone(),
                class: ProbeClass::Readiness,
                spec,
            });
        }

        if let Some(probe) = &container.liveness_probe
            && let Some(spec) =
                build_probe_spec(pod, &container.name, ProbeClass::Liveness, probe, pod_ip)?
        {
            registrations.push(ProbeRegistration {
                container: container.name.clone(),
                class: ProbeClass::Liveness,
                spec,
            });
        }

        if let Some(probe) = &container.startup_probe
            && let Some(spec) =
                build_probe_spec(pod, &container.name, ProbeClass::Startup, probe, pod_ip)?
        {
            registrations.push(ProbeRegistration {
                container: container.name.clone(),
                class: ProbeClass::Startup,
                spec,
            });
        }
    }

    Ok(registrations)
}

pub fn register_pod_probes(pod_name: &str, registrations: Vec<ProbeRegistration>) -> Result<()> {
    if registrations.is_empty() {
        debug!(pod = pod_name, "no probes to register");
        return Ok(());
    }

    // Capture the current runtime handle and use it explicitly when spawning
    // background cleanup tasks. This makes the dependency on an active
    // Tokio runtime explicit and avoids relying on `tokio::spawn`'s global
    // runtime resolution semantics.
    let runtime = tokio::runtime::Handle::try_current()
        .context("probe registration requires an active Tokio runtime")?;

    let mut managed = Vec::with_capacity(registrations.len());
    for registration in registrations {
        let handle = spawn_probe(registration.spec);
        let status_rx = handle.status();
        managed.push(ManagedProbe {
            container: registration.container,
            class: registration.class,
            handle,
            status_rx,
        });
    }

    let old = {
        let mut registry = match PROBE_REGISTRY.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                warn!(
                    pod = pod_name,
                    "probe registry mutex was poisoned; recovering inner data"
                );
                poisoned.into_inner()
            }
        };
        registry.insert(pod_name.to_string(), PodProbes { probes: managed })
    };

    if let Some(previous) = old {
        // Spawn the shutdown task onto the same runtime we validated above.
        runtime.spawn(async move {
            previous.shutdown().await;
        });
    }

    Ok(())
}

pub async fn deregister_pod_probes(pod_name: &str) {
    let old = {
        let mut registry = PROBE_REGISTRY.lock().unwrap_or_else(|err| err.into_inner());
        registry.remove(pod_name)
    };

    if let Some(previous) = old {
        previous.shutdown().await;
    }
}

pub fn collect_container_statuses(pod_name: &str) -> Vec<ContainerStatus> {
    let registry = PROBE_REGISTRY.lock().unwrap_or_else(|err| err.into_inner());

    let Some(pod_probes) = registry.get(pod_name) else {
        return Vec::new();
    };

    let mut container_map: HashMap<String, ContainerStatus> = HashMap::new();
    for snapshot in pod_probes.snapshots() {
        let entry = container_map
            .entry(snapshot.container.clone())
            .or_insert_with(|| ContainerStatus {
                name: snapshot.container.clone(),
                ..ContainerStatus::default()
            });

        let status = to_container_probe_status(&snapshot.status);
        match snapshot.class {
            ProbeClass::Readiness => entry.readiness_probe = Some(status),
            ProbeClass::Liveness => entry.liveness_probe = Some(status),
            ProbeClass::Startup => entry.startup_probe = Some(status),
        }
    }

    container_map.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use tokio::time::{Duration, sleep, timeout};

    use common::{ContainerSpec, ExecAction, HttpGetAction, ObjectMeta, PodSpec, PodStatus};
    use std::collections::HashMap;

    async fn wait_for_state(
        mut rx: watch::Receiver<ProbeStatus>,
        target: ProbeState,
        limit: Duration,
    ) -> ProbeStatus {
        timeout(limit, async {
            loop {
                let current = rx.borrow().clone();
                if current.state == target {
                    return current;
                }
                rx.changed()
                    .await
                    .expect("status channel closed unexpectedly");
            }
        })
        .await
        .expect("probe state did not reach target in time")
    }

    #[tokio::test]
    async fn exec_probe_reports_ready_after_success_threshold() -> Result<()> {
        const PERIOD: Duration = Duration::from_millis(50);
        let config = ProbeConfig {
            initial_delay: Duration::from_millis(0),
            period: PERIOD,
            timeout: Duration::from_secs(1),
            success_threshold: 2,
            failure_threshold: 3,
        };

        let spec = ProbeSpec::new(
            "exec-success",
            ProbeKind::Exec {
                command: vec!["/bin/sh".into(), "-c".into(), "exit 0".into()],
            },
        )
        .with_config(config.clone());

        let handle = spawn_probe(spec);
        let status_rx = handle.status();
        let status = wait_for_state(status_rx, ProbeState::Ready, Duration::from_secs(2)).await;

        assert!(status.consecutive_successes >= config.success_threshold);
        assert_eq!(status.last_error, None);

        handle.shutdown().await;
        Ok(())
    }

    #[tokio::test]
    async fn exec_probe_reports_failing_after_failure_threshold() -> Result<()> {
        const PERIOD: Duration = Duration::from_millis(50);
        let config = ProbeConfig {
            initial_delay: Duration::from_millis(0),
            period: PERIOD,
            timeout: Duration::from_millis(200),
            success_threshold: 1,
            failure_threshold: 2,
        };

        let spec = ProbeSpec::new(
            "exec-fail",
            ProbeKind::Exec {
                command: vec!["/bin/sh".into(), "-c".into(), "exit 1".into()],
            },
        )
        .with_config(config.clone());

        let handle = spawn_probe(spec);
        let status_rx = handle.status();
        let status = wait_for_state(status_rx, ProbeState::Failing, Duration::from_secs(2)).await;

        assert!(status.consecutive_failures >= config.failure_threshold);
        assert!(status.last_error.is_some());

        handle.shutdown().await;
        Ok(())
    }

    #[test]
    fn build_probe_registrations_maps_actions() -> Result<()> {
        let pod = pod_with_probes(Some("example.com"));
        let regs = build_probe_registrations(&pod, "10.0.0.2")?;

        assert_eq!(regs.len(), 2);

        let readiness = regs
            .iter()
            .find(|r| r.class == ProbeClass::Readiness)
            .expect("missing readiness probe");
        match &readiness.spec.kind {
            ProbeKind::Exec { command } => {
                assert_eq!(
                    command,
                    &vec![
                        "/bin/sh".to_string(),
                        "-c".to_string(),
                        "exit 0".to_string(),
                    ]
                );
            }
            other => panic!("unexpected readiness probe kind: {other:?}"),
        }

        let liveness = regs
            .iter()
            .find(|r| r.class == ProbeClass::Liveness)
            .expect("missing liveness probe");
        match &liveness.spec.kind {
            ProbeKind::HttpGet { host, port, path } => {
                assert_eq!(host, "example.com");
                assert_eq!(*port, 8080);
                assert_eq!(path, "/healthz");
            }
            other => panic!("unexpected liveness probe kind: {other:?}"),
        }

        Ok(())
    }

    #[test]
    fn build_probe_registration_requires_host_or_ip() {
        let pod = pod_with_probes(None);
        let err = build_probe_registrations(&pod, "")
            .err()
            .expect("expected host validation error");
        assert!(err.to_string().contains("requires host or pod IP"));
    }

    #[tokio::test]
    async fn collect_statuses_reflects_probe_results() -> Result<()> {
        let pod_name = "collect-status-demo";
        let registration = ProbeRegistration {
            container: "demo-container".to_string(),
            class: ProbeClass::Readiness,
            spec: ProbeSpec::new(
                format!("{pod_name}/demo-container/readiness"),
                ProbeKind::Exec {
                    command: vec!["/bin/sh".into(), "-c".into(), "exit 0".into()],
                },
            )
            .with_config(ProbeConfig {
                period: Duration::from_millis(50),
                timeout: Duration::from_secs(1),
                ..ProbeConfig::default()
            }),
        };

        register_pod_probes(pod_name, vec![registration])?;

        let mut ready = false;
        for _ in 0..40 {
            let statuses = collect_container_statuses(pod_name);
            if let Some(status) = statuses.iter().find(|s| s.name == "demo-container") {
                if let Some(probe) = &status.readiness_probe {
                    if probe.state == ProbeCondition::Ready {
                        ready = true;
                        break;
                    }
                }
            }
            sleep(Duration::from_millis(50)).await;
        }

        deregister_pod_probes(pod_name).await;

        assert!(ready, "readiness probe did not report Ready in time");
        Ok(())
    }

    fn pod_with_probes(http_host: Option<&str>) -> PodTask {
        let mut labels = HashMap::new();
        labels.insert("app".to_string(), "demo".to_string());

        PodTask {
            api_version: "v1".to_string(),
            kind: "Pod".to_string(),
            metadata: ObjectMeta {
                name: "demo".to_string(),
                namespace: "default".to_string(),
                annotations: HashMap::new(),
                ..Default::default()
            },
            spec: PodSpec {
                node_name: None,
                containers: vec![ContainerSpec {
                    name: "demo-container".to_string(),
                    image: "busybox".to_string(),
                    ports: vec![],
                    args: vec![],
                    resources: None,
                    liveness_probe: Some(Probe {
                        action: Some(ProbeAction::HttpGet(HttpGetAction {
                            path: "/healthz".to_string(),
                            port: 8080,
                            host: http_host.map(str::to_string),
                        })),
                        ..Probe::default()
                    }),
                    readiness_probe: Some(Probe {
                        action: Some(ProbeAction::Exec(ExecAction {
                            command: vec![
                                "/bin/sh".to_string(),
                                "-c".to_string(),
                                "exit 0".to_string(),
                            ],
                        })),
                        ..Probe::default()
                    }),
                    startup_probe: None,
                }],
                init_containers: vec![],
                tolerations: vec![],
            },
            status: PodStatus::default(),
        }
    }
}
