use std::{sync::Arc, time::Duration};

use anyhow::anyhow;
use common::{ExecAction, HttpGetAction, PodTask, ProbeAction, RksMessage, TcpSocketAction};
use dashmap::DashMap;
use libcontainer::syscall::syscall::create_syscall;
use libruntime::rootpath;
use tokio::{select, sync::OnceCell};
use tracing::{info, warn};
use uuid::Uuid;

use crate::{
    commands::pod::{PodInfo, TLSConnectionArgs},
    daemon::status::probe::prober::{
        ExecProber, HttpGetProber, ProbeConfig, Prober, TcpSocketProber,
    },
    quic::client::{Cli, QUICClient},
};

pub static PROBE_MANAGER: OnceCell<Arc<ProbeManager>> = OnceCell::const_new();

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeClass {
    Liveness,
    Readiness,
    Startup,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ProbeResultType {
    Success,
    Failure,
    #[default]
    Unknown,
}

#[derive(Clone)]
pub struct ProbeResult {
    pub pod_id: String,
    pub container_id: String,
    pub result: ProbeResultType,
}
impl ProbeResult {
    pub fn new_success(pod_id: impl Into<String>, container_id: impl Into<String>) -> Self {
        Self {
            pod_id: pod_id.into(),
            container_id: container_id.into(),
            result: ProbeResultType::Success,
        }
    }

    pub fn new_failure(pod_id: impl Into<String>, container_id: impl Into<String>) -> Self {
        Self {
            pod_id: pod_id.into(),
            container_id: container_id.into(),
            result: ProbeResultType::Failure,
        }
    }

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

pub struct ProbeResultManager {
    result_cache: DashMap<String, ProbeResult>,
    result_tx: tokio::sync::broadcast::Sender<ProbeResult>,
}

impl Default for ProbeResultManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ProbeResultManager {
    pub fn new() -> Self {
        let (tx, _rx) = tokio::sync::broadcast::channel(32);
        Self {
            result_cache: DashMap::new(),
            result_tx: tx,
        }
    }

    /// Store a probe result and notify subscribers if probe result has changed.
    pub fn set(&self, result: ProbeResult) {
        let key = result.container_id.clone();
        let notify = match self.result_cache.get(&key) {
            Some(existing) => existing.result != result.result,
            None => true,
        };
        if notify {
            self.result_cache.insert(key, result.clone());
            let _ = self.result_tx.send(result);
        }
    }

    /// Subscribe to probe result updates.
    pub fn updates(&self) -> tokio::sync::broadcast::Receiver<ProbeResult> {
        self.result_tx.subscribe()
    }
}

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

impl ProbeManager {
    pub fn new() -> Self {
        Self {
            liveness_results: Arc::new(ProbeResultManager::new()),
            readiness_results: Arc::new(ProbeResultManager::new()),
            startup_results: Arc::new(ProbeResultManager::new()),
            probe_workers: DashMap::new(),
        }
    }

    pub async fn add_pod(&self, pod: &PodTask, pod_ip: &str) -> anyhow::Result<()> {
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
                self.probe_workers
                    .entry(pod.metadata.name.clone())
                    .or_default()
                    .push(worker);
            }
        }

        Ok(())
    }

    pub async fn remove_pod(&self, pod_name: &str) {
        self.probe_workers.remove(pod_name);
    }

    pub fn liveness_results(&self) -> Arc<ProbeResultManager> {
        self.liveness_results.clone()
    }

    pub fn readiness_results(&self) -> Arc<ProbeResultManager> {
        self.readiness_results.clone()
    }

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
            period: Duration::from_secs(probe.period_seconds.unwrap_or(10) as u64),
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
        None
    }
}

pub struct ProbeWorker {
    results_manager: Arc<ProbeResultManager>,
    prober: Arc<dyn Prober + Send + Sync>,
    pod_id: String,
    container_id: String,
    handle: Option<tokio::task::JoinHandle<()>>,
    stop_signal_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl ProbeWorker {
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

    pub async fn run(&mut self) {
        let results_manager = self.results_manager.clone();
        let prober = self.prober.clone();
        let config = prober.config().clone();
        let pod_id = self.pod_id.clone();
        let container_id = self.container_id.clone();
        let (stop_tx, mut stop_rx) = tokio::sync::oneshot::channel();

        self.stop_signal_tx.replace(stop_tx);
        self.handle = Some(tokio::spawn(async move {
            if !config.initial_delay.is_zero() {
                let delay = tokio::time::sleep(config.initial_delay);
                tokio::pin!(delay);
                select! {
                    _ = &mut delay => {}
                    _ = &mut stop_rx => {
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
                                }
                                tracing::warn!(timeout = ?config.timeout, "[Probe] probe timed out");
                            }
                        }
                    }
                    _ = &mut stop_rx => {
                        break;
                    }
                }
            }
        }));
    }

    pub fn stop(&mut self) {
        if let Some(stop_tx) = self.stop_signal_tx.take() {
            let _ = stop_tx.send(());
        }
        self.handle.take();
    }
}

impl Drop for ProbeWorker {
    fn drop(&mut self) {
        self.stop();
    }
}

pub async fn restore_existing_probes(
    server_addr: &str,
    tls_cfg: Arc<TLSConnectionArgs>,
    probe_manager: Arc<ProbeManager>,
) -> anyhow::Result<()> {
    let client = QUICClient::<Cli>::connect(server_addr.to_string(), &tls_cfg).await?;
    client.send_msg(&RksMessage::ListPod).await?;
    let pods = match client.fetch_msg().await? {
        RksMessage::ListPodRes(pods) => pods,
        msg => anyhow::bail!("unexpected response {msg:?}"),
    };

    let root_path = rootpath::determine(None, &*create_syscall())?;
    let mut restored = 0usize;

    for pod in pods {
        if PodInfo::load(&root_path, &pod.metadata.name).is_err() {
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
}
