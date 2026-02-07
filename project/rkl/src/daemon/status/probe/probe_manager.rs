//! Container health probe management.
//!
//! This module manages the lifecycle of container health probes (liveness, readiness, startup).
//! [`ProbeManager`] registers per-pod probe workers, each running a [`Prober`] on a periodic timer.
//! Results flow through [`ProbeResultManager`] which caches the latest result per container and
//! broadcasts changes to subscribers (typically the [`crate::daemon::pod_worker::PodWorker`]).

use std::{sync::Arc, time::Duration};

use anyhow::anyhow;
use common::{ExecAction, HttpGetAction, PodTask, ProbeAction, RksMessage, TcpSocketAction};
use dashmap::DashMap;
use libcontainer::syscall::syscall::create_syscall;
use libruntime::rootpath;
use tokio::{select, sync::OnceCell};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::{
    commands::pod::{PodInfo, TLSConnectionArgs},
    daemon::status::probe::prober::{
        ExecProber, HttpGetProber, ProbeConfig, Prober, TcpSocketProber,
    },
    quic::client::{Cli, QUICClient},
};

/// Global singleton [`ProbeManager`], initialized once by the daemon.
pub static PROBE_MANAGER: OnceCell<Arc<ProbeManager>> = OnceCell::const_new();

/// Classifies a probe as liveness, readiness, or startup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeClass {
    /// Probe that determines if a container is alive; failure may trigger a restart.
    Liveness,
    /// Probe that determines if a container is ready to serve traffic.
    Readiness,
    /// Probe that gates liveness/readiness checks until the container has started.
    Startup,
}

/// Outcome of a single probe execution.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ProbeResultType {
    /// Probe succeeded.
    Success,
    /// Probe failed.
    Failure,
    /// Probe result is not yet known (default).
    #[default]
    Unknown,
}

/// A probe outcome tied to a specific pod and container.
#[derive(Clone)]
pub struct ProbeResult {
    /// Pod ID for this probe result.
    pub pod_id: String,
    /// Container name used by status updates and restart handling.
    pub container_id: String,
    /// The outcome of the probe execution.
    pub result: ProbeResultType,
}
impl ProbeResult {
    /// Creates a successful probe result.
    pub fn new_success(pod_id: impl Into<String>, container_id: impl Into<String>) -> Self {
        Self {
            pod_id: pod_id.into(),
            container_id: container_id.into(),
            result: ProbeResultType::Success,
        }
    }

    /// Creates a failed probe result.
    pub fn new_failure(pod_id: impl Into<String>, container_id: impl Into<String>) -> Self {
        Self {
            pod_id: pod_id.into(),
            container_id: container_id.into(),
            result: ProbeResultType::Failure,
        }
    }

    /// Creates an unknown probe result.
    #[allow(unused)]
    pub fn new_unknown(pod_id: impl Into<String>, container_id: impl Into<String>) -> Self {
        Self {
            pod_id: pod_id.into(),
            container_id: container_id.into(),
            result: ProbeResultType::Unknown,
        }
    }
}

impl std::fmt::Debug for ProbeResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProbeResult")
            .field("pod_id", &self.pod_id)
            .field("container_id", &self.container_id)
            .field("result", &self.result)
            .finish()
    }
}

/// Caches the latest probe result per container and broadcasts changes to subscribers.
pub struct ProbeResultManager {
    result_cache: DashMap<String, ProbeResult>,
    result_tx: tokio::sync::broadcast::Sender<ProbeResult>,
}

impl Default for ProbeResultManager {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ProbeResultManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProbeResultManager")
            .field(
                "result_cache",
                &self
                    .result_cache
                    .iter()
                    .map(|v| (v.key().clone(), v.value().result.clone()))
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl ProbeResultManager {
    /// Creates a new empty result manager.
    pub fn new() -> Self {
        let (tx, _rx) = tokio::sync::broadcast::channel(32);
        Self {
            result_cache: DashMap::new(),
            result_tx: tx,
        }
    }

    /// Store a probe result and notify subscribers if probe result has changed.
    pub fn set(&self, result: ProbeResult) {
        let key = format!("{}:{}", result.pod_id, result.container_id);
        let notify = match self.result_cache.get(&key) {
            Some(existing) => existing.result != result.result,
            None => true,
        };
        if notify {
            self.result_cache.insert(key, result.clone());
            let _ = self.result_tx.send(result);
            debug!("[Probe] Probe result changed and broadcasted");
        } else {
            debug!("[Probe] Probe result unchanged; suppressing broadcast");
        }
    }

    /// Subscribe to probe result updates.
    pub fn updates(&self) -> tokio::sync::broadcast::Receiver<ProbeResult> {
        self.result_tx.subscribe()
    }

    /// Remove cached results whose key starts with `pod_id:`.
    pub fn remove_by_pod(&self, pod_id: &str) {
        let prefix = format!("{pod_id}:");
        self.result_cache.retain(|key, _| !key.starts_with(&prefix));
    }
}

/// Orchestrates all probe workers for all pods.
pub struct ProbeManager {
    liveness_results: Arc<ProbeResultManager>,
    readiness_results: Arc<ProbeResultManager>,
    startup_results: Arc<ProbeResultManager>,
    probe_workers: DashMap<String, Vec<ProbeWorker>>,
}

impl Default for ProbeManager {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ProbeManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProbeManager")
            .field("liveness_results", &self.liveness_results)
            .field("readiness_results", &self.readiness_results)
            .field("startup_results", &self.startup_results)
            .field(
                "probe_workers",
                &self
                    .probe_workers
                    .iter()
                    .map(|entry| {
                        let workers = entry
                            .value()
                            .iter()
                            .map(|worker| format!("{worker:?}"))
                            .collect::<Vec<_>>();
                        (entry.key().clone(), workers)
                    })
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl ProbeManager {
    /// Creates a new [`ProbeManager`] with empty result managers.
    pub fn new() -> Self {
        Self {
            liveness_results: Arc::new(ProbeResultManager::new()),
            readiness_results: Arc::new(ProbeResultManager::new()),
            startup_results: Arc::new(ProbeResultManager::new()),
            probe_workers: DashMap::new(),
        }
    }

    /// Creates and starts probe workers for all probes defined in the pod spec.
    pub async fn add_pod(&self, pod: &PodTask, pod_ip: &str) -> anyhow::Result<()> {
        debug!(
            pod_uid = %pod.metadata.uid,
            pod_name = %pod.metadata.name,
            pod_namespace = %pod.metadata.namespace,
            pod_ip,
            container_count = pod.spec.containers.len(),
            "[ProbeManager] add_pod called"
        );
        if self.probe_workers.get(&pod.metadata.name).is_some() {
            return Err(anyhow!(
                "[ProbeManager] Probes for pod {} already exist",
                pod.metadata.name
            ));
        }

        let probes = pod.spec.containers.iter().flat_map(|container| {
            let mut probes = Vec::new();
            if let Some(probe) = &container.liveness_probe {
                probes.push((probe, ProbeClass::Liveness, container.name.clone()));
            }
            if let Some(probe) = &container.readiness_probe {
                probes.push((probe, ProbeClass::Readiness, container.name.clone()));
            }
            if let Some(probe) = &container.startup_probe {
                probes.push((probe, ProbeClass::Startup, container.name.clone()));
            }
            probes
        });

        for (probe_spec, probe_class, container_name) in probes {
            if let Some(prober) = create_prober_from_spec(
                probe_spec,
                pod.metadata.uid,
                pod.metadata.name.clone(),
                container_name.clone(),
                pod_ip,
            ) {
                let results_manager = match probe_class {
                    ProbeClass::Liveness => self.liveness_results.clone(),
                    ProbeClass::Readiness => self.readiness_results.clone(),
                    ProbeClass::Startup => self.startup_results.clone(),
                };
                let mut worker = ProbeWorker::new(
                    results_manager,
                    prober,
                    pod.metadata.uid,
                    container_name.clone(),
                );
                worker.run().await;
                debug!(
                    pod_uid = %pod.metadata.uid,
                    pod_name = %pod.metadata.name,
                    container_name,
                    probe_class = ?probe_class,
                    "[ProbeManager] Probe worker started"
                );
                self.probe_workers
                    .entry(pod.metadata.name.clone())
                    .or_default()
                    .push(worker);
            }
        }
        let worker_count = self
            .probe_workers
            .get(&pod.metadata.name)
            .map(|workers| workers.len())
            .unwrap_or(0);
        debug!(
            pod_uid = %pod.metadata.uid,
            pod_name = %pod.metadata.name,
            worker_count,
            "[ProbeManager] add_pod completed"
        );

        Ok(())
    }

    /// Stops and removes all probe workers for the given pod.
    pub async fn remove_pod(&self, pod_name: &str) {
        debug!(pod_name, "[ProbeManager] Removing pod probe workers");
        if let Some((_, workers)) = self.probe_workers.remove(pod_name) {
            for worker in &workers {
                self.liveness_results.remove_by_pod(&worker.pod_id);
                self.readiness_results.remove_by_pod(&worker.pod_id);
                self.startup_results.remove_by_pod(&worker.pod_id);
            }
            // workers are dropped here, triggering ProbeWorker::drop → stop()
        }
    }

    /// Returns the shared liveness result manager.
    pub fn liveness_results(&self) -> Arc<ProbeResultManager> {
        self.liveness_results.clone()
    }

    /// Returns the shared readiness result manager.
    pub fn readiness_results(&self) -> Arc<ProbeResultManager> {
        self.readiness_results.clone()
    }

    /// Returns the shared startup result manager.
    pub fn startup_results(&self) -> Arc<ProbeResultManager> {
        self.startup_results.clone()
    }
}

fn create_prober_from_spec(
    probe: &common::Probe,
    pod_id: Uuid,
    pod_name: String,
    container_name: String,
    pod_ip: &str,
) -> Option<Arc<dyn Prober + Send + Sync>> {
    if let Some(action) = &probe.action {
        let config = ProbeConfig {
            pod_id,
            pod_name,
            container_name,
            initial_delay: Duration::from_secs(probe.initial_delay_seconds.unwrap_or(0) as u64),
            timeout: Duration::from_secs(probe.timeout_seconds.unwrap_or(1) as u64),
            period: Duration::from_secs(probe.period_seconds.unwrap_or(10).max(1) as u64),
            success_threshold: probe.success_threshold.unwrap_or(1),
            failure_threshold: probe.failure_threshold.unwrap_or(3),
        };
        match action {
            ProbeAction::Exec(ExecAction { command }) => {
                Some(Arc::new(ExecProber::new(command.clone(), config.clone())))
            }
            ProbeAction::HttpGet(HttpGetAction { host, port, path }) => {
                Some(Arc::new(HttpGetProber::new(
                    host.clone().unwrap_or(pod_ip.to_string()),
                    *port,
                    path.clone(),
                    config.clone(),
                )))
            }
            ProbeAction::TcpSocket(TcpSocketAction { host, port }) => {
                Some(Arc::new(TcpSocketProber::new(
                    host.clone().unwrap_or(pod_ip.to_string()),
                    *port,
                    config.clone(),
                )))
            }
        }
    } else {
        debug!(
            pod_id = %pod_id,
            pod_name,
            container_name,
            "[ProbeManager] Probe spec has no action; skipping prober creation"
        );
        None
    }
}

/// Runs a single [`Prober`] on a timer, publishing results to a [`ProbeResultManager`].
pub struct ProbeWorker {
    results_manager: Arc<ProbeResultManager>,
    prober: Arc<dyn Prober + Send + Sync>,
    pod_id: String,
    container_id: String,
    handle: Option<tokio::task::JoinHandle<()>>,
    stop_signal_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl std::fmt::Debug for ProbeWorker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProbeWorker")
            .field("pod_id", &self.pod_id)
            .field("container_id", &self.container_id)
            .finish()
    }
}

impl ProbeWorker {
    /// Creates a new probe worker (does not start it).
    pub fn new(
        results_manager: Arc<ProbeResultManager>,
        prober: Arc<dyn Prober + Send + Sync>,
        pod_id: Uuid,
        container_id: String,
    ) -> Self {
        Self {
            results_manager,
            prober,
            pod_id: pod_id.to_string(),
            container_id,
            handle: None,
            stop_signal_tx: None,
        }
    }

    /// Starts the probe loop: waits for initial delay, then probes periodically.
    pub async fn run(&mut self) {
        if let Some(handle) = &self.handle {
            if !handle.is_finished() {
                warn!(
                    pod_id = %self.pod_id,
                    container_id = %self.container_id,
                    "[Probe] run() called while already running; ignoring."
                );
                return;
            }
            self.handle = None;
            self.stop_signal_tx = None;
        }

        let results_manager = self.results_manager.clone();
        let prober = self.prober.clone();
        let config = prober.config().clone();
        let pod_id = self.pod_id.clone();
        let container_id = self.container_id.clone();
        let (stop_tx, mut stop_rx) = tokio::sync::oneshot::channel();

        self.stop_signal_tx.replace(stop_tx);
        self.handle = Some(tokio::spawn(async move {
            debug!(
                pod_id,
                container_id,
                initial_delay = ?config.initial_delay,
                period = ?config.period,
                timeout = ?config.timeout,
                success_threshold = config.success_threshold,
                failure_threshold = config.failure_threshold,
                "[Probe] Probe worker started"
            );
            if !config.initial_delay.is_zero() {
                let delay = tokio::time::sleep(config.initial_delay);
                tokio::pin!(delay);
                select! {
                    _ = &mut delay => {}
                    _ = &mut stop_rx => {
                        debug!(pod_id, container_id, "[Probe] Probe worker stopped during initial delay");
                        return;
                    }
                }
            }

            let mut interval = tokio::time::interval(config.period);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            let mut success_count = 0u32;
            let mut failure_count = 0u32;
            loop {
                select! {
                    _ = interval.tick() => {
                        match tokio::time::timeout(config.timeout, prober.probe()).await {
                            Ok(Ok(())) => {
                                success_count = success_count.saturating_add(1);
                                failure_count = 0;
                                if success_count >= config.success_threshold {
                                    results_manager.set(ProbeResult::new_success(
                                        pod_id.clone(),
                                        container_id.clone(),
                                    ));
                                    debug!(
                                        pod_id,
                                        container_id,
                                        success_count,
                                        "[Probe] Success threshold reached"
                                    );
                                }
                            }
                            Ok(Err(e)) => {
                                failure_count = failure_count.saturating_add(1);
                                success_count = 0;
                                if failure_count >= config.failure_threshold {
                                    results_manager.set(ProbeResult::new_failure(
                                        pod_id.clone(),
                                        container_id.clone(),
                                    ));
                                    debug!(
                                        pod_id,
                                        container_id,
                                        failure_count,
                                        "[Probe] Failure threshold reached after probe error"
                                    );
                                }
                                tracing::warn!(error = %e, "[Probe] probe failed");
                            }
                            Err(_) => {
                                failure_count = failure_count.saturating_add(1);
                                success_count = 0;
                                if failure_count >= config.failure_threshold {
                                    results_manager.set(ProbeResult::new_failure(
                                        pod_id.clone(),
                                        container_id.clone(),
                                    ));
                                    debug!(
                                        pod_id,
                                        container_id,
                                        failure_count,
                                        "[Probe] Failure threshold reached after probe timeout"
                                    );
                                }
                                tracing::warn!(timeout = ?config.timeout, "[Probe] probe timed out");
                            }
                        }
                    }
                    _ = &mut stop_rx => {
                        debug!(pod_id, container_id, "[Probe] Probe worker received stop signal");
                        break;
                    }
                }
            }
            debug!(pod_id, container_id, "[Probe] Probe worker exited");
        }));
    }

    /// Signals the probe loop to stop.
    pub fn stop(&mut self) {
        if let Some(stop_tx) = self.stop_signal_tx.take() {
            let _ = stop_tx.send(());
        }
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

impl Drop for ProbeWorker {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Re-registers probes for pods that already exist on the server (called on daemon restart).
pub async fn restore_existing_probes(
    server_addr: &str,
    tls_cfg: Arc<TLSConnectionArgs>,
    probe_manager: Arc<ProbeManager>,
) -> anyhow::Result<()> {
    debug!(
        server_addr,
        "[daemon] Restoring existing probes from server pod list"
    );
    let client = QUICClient::<Cli>::connect(server_addr.to_string(), &tls_cfg).await?;
    client.send_msg(&RksMessage::ListPod).await?;
    let pods = match client.fetch_msg().await? {
        RksMessage::ListPodRes(pods) => pods,
        msg => anyhow::bail!("unexpected response {msg:?}"),
    };
    debug!(
        pod_count = pods.len(),
        "[daemon] Loaded pods for probe restoration"
    );

    let root_path = rootpath::determine(None, &*create_syscall())?;
    let mut restored = 0usize;

    for pod in pods {
        if PodInfo::load(&root_path, &pod.metadata.name).is_err() {
            debug!(
                pod = %pod.metadata.name,
                namespace = %pod.metadata.namespace,
                "[daemon] skipping probe restore: pod not found in local runtime"
            );
            continue;
        }

        let pod_ip = match pod.status.pod_ip.clone() {
            Some(ip) if !ip.is_empty() => ip,
            _ => {
                warn!(
                    pod = %pod.metadata.name,
                    "[daemon] skipping probe restore: missing pod IP"
                );
                continue;
            }
        };

        if let Err(e) = probe_manager.add_pod(&pod, &pod_ip).await {
            warn!(
                pod = %pod.metadata.name,
                error = %e,
                "[daemon] failed to restore probes for pod"
            );
        } else {
            restored += 1;
            debug!(
                pod = %pod.metadata.name,
                namespace = %pod.metadata.namespace,
                "[daemon] restored probes for pod"
            );
        }
    }

    if restored > 0 {
        info!("[daemon] restored probes for {restored} pods");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::daemon::status::probe::prober::{ProbeConfig, ProbeKind};

    use super::*;
    use common::{
        ContainerSpec, ExecAction, HttpGetAction, ObjectMeta, PodSpec, PodStatus, PodTask, Probe,
        ProbeAction, RestartPolicy, TcpSocketAction,
    };
    use std::sync::Arc;
    use tokio::time::{Duration, timeout};
    use uuid::Uuid;

    struct StaticProber {
        success: bool,
        config: ProbeConfig,
    }

    #[async_trait::async_trait]
    impl Prober for StaticProber {
        async fn probe(&self) -> anyhow::Result<()> {
            if self.success {
                Ok(())
            } else {
                Err(anyhow!("probe failed"))
            }
        }

        fn probe_kind(&self) -> ProbeKind {
            ProbeKind::Exec
        }

        fn config(&self) -> &ProbeConfig {
            &self.config
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    fn test_probe_config() -> ProbeConfig {
        ProbeConfig {
            initial_delay: Duration::from_millis(0),
            period: Duration::from_millis(10),
            timeout: Duration::from_millis(50),
            success_threshold: 1,
            failure_threshold: 1,
            ..Default::default()
        }
    }

    fn test_probe() -> Probe {
        Probe {
            action: Some(ProbeAction::TcpSocket(TcpSocketAction {
                host: None,
                port: 1234,
            })),
            initial_delay_seconds: Some(3600),
            period_seconds: Some(3600),
            ..Default::default()
        }
    }

    fn test_pod_task(pod_name: &str) -> PodTask {
        let mut metadata = ObjectMeta::default();
        metadata.name = pod_name.to_string();

        let probe = test_probe();
        PodTask {
            api_version: "v1".to_string(),
            kind: "Pod".to_string(),
            metadata,
            spec: PodSpec {
                node_name: None,
                containers: vec![
                    ContainerSpec {
                        name: "app".to_string(),
                        image: "busybox".to_string(),
                        ports: vec![],
                        args: vec![],
                        resources: None,
                        liveness_probe: Some(probe.clone()),
                        readiness_probe: Some(probe.clone()),
                        startup_probe: None,
                        security_context: None,
                        env: None,
                        volume_mounts: None,
                        command: None,
                        working_dir: None,
                    },
                    ContainerSpec {
                        name: "sidecar".to_string(),
                        image: "busybox".to_string(),
                        ports: vec![],
                        args: vec![],
                        resources: None,
                        liveness_probe: None,
                        readiness_probe: None,
                        startup_probe: Some(probe),
                        security_context: None,
                        env: None,
                        volume_mounts: None,
                        command: None,
                        working_dir: None,
                    },
                ],
                init_containers: vec![],
                tolerations: vec![],
                affinity: None,
                restart_policy: RestartPolicy::Always,
            },
            status: PodStatus::default(),
        }
    }

    #[tokio::test]
    async fn add_pod_registers_probe_workers() {
        let manager = ProbeManager::new();
        let pod = test_pod_task("demo-pod");

        manager.add_pod(&pod, "127.0.0.1").await.expect("add_pod");

        let workers = manager.probe_workers.get("demo-pod").expect("workers");
        assert_eq!(workers.len(), 3);
        let mut app_count = 0;
        let mut sidecar_count = 0;
        for worker in workers.iter() {
            match worker.container_id.as_str() {
                "app" => app_count += 1,
                "sidecar" => sidecar_count += 1,
                other => panic!("unexpected container id {other}"),
            }
        }
        assert_eq!(app_count, 2);
        assert_eq!(sidecar_count, 1);
    }

    #[tokio::test]
    async fn add_pod_rejects_duplicate() {
        let manager = ProbeManager::new();
        let pod = test_pod_task("dup-pod");

        manager
            .add_pod(&pod, "127.0.0.1")
            .await
            .expect("first add_pod");

        let err = manager
            .add_pod(&pod, "127.0.0.1")
            .await
            .expect_err("duplicate add_pod should error");
        assert!(
            err.to_string()
                .contains("[ProbeManager] Probes for pod dup-pod already exist")
        );
    }

    #[tokio::test]
    async fn remove_pod_clears_workers() {
        let manager = ProbeManager::new();
        let pod = test_pod_task("remove-pod");

        manager.add_pod(&pod, "127.0.0.1").await.expect("add_pod");
        manager.remove_pod("remove-pod").await;

        assert!(manager.probe_workers.get("remove-pod").is_none());
    }

    #[tokio::test]
    async fn probe_result_manager_emits_on_change() {
        let manager = ProbeResultManager::new();
        let mut rx = manager.updates();

        manager.set(ProbeResult::new_success("pod1", "c1"));
        let first = timeout(Duration::from_millis(100), rx.recv())
            .await
            .expect("timeout")
            .expect("recv");
        assert_eq!(first.result, ProbeResultType::Success);
        assert_eq!(first.container_id, "c1");

        manager.set(ProbeResult::new_success("pod1", "c1"));
        assert!(rx.try_recv().is_err());

        manager.set(ProbeResult::new_failure("pod1", "c1"));
        let second = timeout(Duration::from_millis(100), rx.recv())
            .await
            .expect("timeout")
            .expect("recv");
        assert_eq!(second.result, ProbeResultType::Failure);
    }

    #[tokio::test]
    async fn probe_result_manager_does_not_mix_same_container_name_across_pods() {
        let manager = ProbeResultManager::new();
        let mut rx = manager.updates();

        manager.set(ProbeResult::new_success("pod1", "app"));
        let first = timeout(Duration::from_millis(100), rx.recv())
            .await
            .expect("timeout")
            .expect("recv");
        assert_eq!(first.pod_id, "pod1");
        assert_eq!(first.result, ProbeResultType::Success);

        manager.set(ProbeResult::new_success("pod2", "app"));
        let second = timeout(Duration::from_millis(100), rx.recv())
            .await
            .expect("timeout")
            .expect("recv");
        assert_eq!(second.pod_id, "pod2");
        assert_eq!(second.result, ProbeResultType::Success);
    }

    #[tokio::test]
    async fn probe_worker_emits_success_result() {
        let manager = Arc::new(ProbeResultManager::new());
        let mut rx = manager.updates();
        let prober = Arc::new(StaticProber {
            success: true,
            config: test_probe_config(),
        });
        let mut worker = ProbeWorker::new(manager, prober, Uuid::nil(), "c1".to_string());

        worker.run().await;
        let result = timeout(Duration::from_millis(200), rx.recv())
            .await
            .expect("timeout")
            .expect("recv");
        assert_eq!(result.result, ProbeResultType::Success);
        assert_eq!(result.container_id, "c1");

        worker.stop();
    }

    #[tokio::test]
    async fn probe_worker_emits_failure_result() {
        let manager = Arc::new(ProbeResultManager::new());
        let mut rx = manager.updates();
        let prober = Arc::new(StaticProber {
            success: false,
            config: test_probe_config(),
        });
        let mut worker = ProbeWorker::new(manager, prober, Uuid::nil(), "c2".to_string());

        worker.run().await;
        let result = timeout(Duration::from_millis(200), rx.recv())
            .await
            .expect("timeout")
            .expect("recv");
        assert_eq!(result.result, ProbeResultType::Failure);
        assert_eq!(result.container_id, "c2");

        worker.stop();
    }

    #[test]
    fn create_prober_from_spec_returns_none_without_action() {
        let probe = Probe::default();
        let prober = create_prober_from_spec(
            &probe,
            Uuid::new_v4(),
            "pod".to_string(),
            "app".to_string(),
            "127.0.0.1",
        );
        assert!(prober.is_none());
    }

    #[test]
    fn create_prober_from_spec_builds_exec_prober() {
        let probe = Probe {
            action: Some(ProbeAction::Exec(ExecAction {
                command: vec!["/bin/true".to_string()],
            })),
            ..Default::default()
        };
        let prober = create_prober_from_spec(
            &probe,
            Uuid::new_v4(),
            "pod".to_string(),
            "app".to_string(),
            "127.0.0.1",
        )
        .expect("prober");
        assert!(matches!(prober.probe_kind(), ProbeKind::Exec));
    }

    #[test]
    fn create_prober_from_spec_builds_http_prober() {
        let probe = Probe {
            action: Some(ProbeAction::HttpGet(HttpGetAction {
                host: None,
                port: 8080,
                path: "/health".to_string(),
            })),
            ..Default::default()
        };
        let prober = create_prober_from_spec(
            &probe,
            Uuid::new_v4(),
            "pod".to_string(),
            "app".to_string(),
            "127.0.0.1",
        )
        .expect("prober");
        assert!(matches!(prober.probe_kind(), ProbeKind::HttpGet));
        let prober = prober
            .as_any()
            .downcast_ref::<HttpGetProber>()
            .expect("http prober");
        assert_eq!(prober.host(), "127.0.0.1");
        assert_eq!(prober.port(), 8080);
        assert_eq!(prober.path(), "/health");
    }

    #[test]
    fn create_prober_from_spec_builds_tcp_prober() {
        let probe = Probe {
            action: Some(ProbeAction::TcpSocket(TcpSocketAction {
                host: None,
                port: 9090,
            })),
            ..Default::default()
        };
        let prober = create_prober_from_spec(
            &probe,
            Uuid::new_v4(),
            "pod".to_string(),
            "app".to_string(),
            "127.0.0.1",
        )
        .expect("prober");
        assert!(matches!(prober.probe_kind(), ProbeKind::TcpSocket));
        let prober = prober
            .as_any()
            .downcast_ref::<TcpSocketProber>()
            .expect("tcp prober");
        assert_eq!(prober.host(), "127.0.0.1");
        assert_eq!(prober.port(), 9090);
    }

    #[test]
    fn create_prober_from_spec_clamps_zero_period_to_one_second() {
        let probe = Probe {
            action: Some(ProbeAction::TcpSocket(TcpSocketAction {
                host: None,
                port: 9090,
            })),
            period_seconds: Some(0),
            ..Default::default()
        };
        let prober = create_prober_from_spec(
            &probe,
            Uuid::new_v4(),
            "pod".to_string(),
            "app".to_string(),
            "127.0.0.1",
        )
        .expect("prober");
        assert_eq!(prober.config().period, Duration::from_secs(1));
    }
}
