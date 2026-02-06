use anyhow::anyhow;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::{collections::HashSet, path::Path, sync::Arc, time::Duration};
use tokio::fs::{File, read_dir};
use tokio::io::AsyncReadExt;
use tracing::{error, warn};

use futures::FutureExt;
use tokio::task::JoinHandle;
use tokio::time::sleep;

use crate::commands::pod;
use crate::task::TaskRunner;
use common::PodTask;

use crate::daemon::probe::{build_probe_registrations, deregister_pod_probes, register_pod_probes};
use crate::daemon::sync_loop::{Event, State, WithEvent};

/// Check and ensure that the pod status is consistent with the requirements every five seconds.
pub struct CheckStaticPodsPeriodically;

impl Event<()> for CheckStaticPodsPeriodically {
    /// Generate a check event every 5 seconds.
    fn listen() -> std::pin::Pin<Box<dyn Future<Output = ()> + Send>> {
        async {
            sleep(Duration::from_secs(5)).await;
        }
        .boxed()
    }
}

/// Ensure that the pod status is consistent with the requirements every five seconds.
/// - If an error occurs, it will be logged and the operation will be stopped.
pub async fn handler(
    state: Arc<State>,
    _data: Box<()>,
    _event: WithEvent<CheckStaticPodsPeriodically>,
) {
    // now, it only moniter the default directory.
    let static_pods = read_pods_from_dir("/etc/rk8s/manifests").await;
    if let Err(e) = static_pods {
        error!("Failed to check static pods: {e}");
        return;
    }

    let pods = static_pods.unwrap();
    let stop_res = stop_removed_pods(state.clone(), &pods).await;
    if let Err(e) = stop_res {
        error!("Failed to stop removed pods: {e}");
        return;
    }
    run_new_pods(state.clone(), pods).await;
}

/// Caculate hash value by its yaml.
fn calculate_hash(p: &PodTask) -> u64 {
    let t = serde_yaml::to_string(p).unwrap();
    // Since the serialization results may be inconsistent, sort lines.
    let mut tmp: Vec<_> = t.split('\n').collect();
    tmp.sort();
    let mut s = DefaultHasher::new();
    tmp.hash(&mut s);
    s.finish()
}

/// Try to start pods that in static pods config directory but not created.
/// - It will be logged as an error if start failed.
async fn run_new_pods(state: Arc<State>, pods: Vec<PodTask>) {
    let mut current_pods = state.pods_mut().await;
    let mut pods_set = HashSet::new();
    for p in &*current_pods {
        let hs = calculate_hash(p);
        pods_set.insert(hs);
    }
    pods.into_iter()
        .filter(|p| !pods_set.contains(&calculate_hash(p)))
        .for_each(|p| {
            let runner = TaskRunner::from_task(p.clone()).unwrap();
            let name = runner.task.metadata.name.clone();
            match pod::sync_run_pod_from_taskrunner(runner) {
                Ok(result) => {
                    match build_probe_registrations(&result.pod_task, &result.pod_ip) {
                        Ok(registrations) => {
                            if let Err(err) =
                                register_pod_probes(&result.pod_task.metadata.name, registrations)
                            {
                                warn!(
                                    "Failed to register probes for static pod {}: {err:?}",
                                    result.pod_task.metadata.name
                                );
                            }
                        }
                        Err(err) => warn!(
                            "Failed to build probes for static pod {}: {err:?}",
                            result.pod_task.metadata.name
                        ),
                    }

                    (*current_pods).push(p);
                }
                Err(e) => {
                    error!("Failed to run pod {}: {e}", name);
                }
            }
        });
}

/// Try to remove pods that not in static pods config directory but created.
async fn stop_removed_pods(state: Arc<State>, pods: &Vec<PodTask>) -> Result<(), anyhow::Error> {
    let mut pods_hash = HashSet::new();
    for p in pods {
        let hs = calculate_hash(p);
        pods_hash.insert(hs);
    }
    // must get write lock to avoid other thread operate pods simitaneously.
    let mut current_pods = state.pods_mut().await;

    let mut errors = Vec::new();
    let mut to_keep = Vec::new();

    let mut deregister_handles: Vec<JoinHandle<()>> = Vec::new();

    for pod in current_pods.drain(..) {
        let hs = calculate_hash(&pod);
        if pods_hash.contains(&hs) {
            to_keep.push(pod);
            continue;
        }

        match pod::standalone::delete_pod(&pod.metadata.name) {
            Ok(_) => {
                // spawn deregistration but keep the JoinHandle so we can observe failures
                let pod_name = pod.metadata.name.clone();
                let handle = tokio::spawn(async move {
                    // ensure panics inside deregister are captured by the task runtime
                    deregister_pod_probes(&pod_name).await;
                });
                deregister_handles.push(handle);
            }
            Err(e) => errors.push(e),
        }
    }

    // Await all deregistration tasks and log if any panicked or failed to run
    for h in deregister_handles {
        if let Err(join_err) = h.await {
            error!("deregister_pod_probes task panicked or was cancelled: {join_err:?}");
        }
    }

    if !errors.is_empty() {
        return Err(anyhow!("{errors:?}"));
    }

    *current_pods = to_keep;
    Ok(())
}

/// Get pods from static pods config directory.
async fn read_pods_from_dir<P: AsRef<Path>>(path: P) -> Result<Vec<PodTask>, anyhow::Error> {
    let mut entries = read_dir(path)
        .await
        .map_err(|e| anyhow!("Failed to read default static pods dir: {e}"))?;
    let mut res = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| anyhow!("Failed to read default static pods dir entries: {e}"))?
    {
        let file_path = entry.path();
        let mut content = String::new();
        let mut file = File::open(&file_path)
            .await
            .map_err(|e| anyhow!("Failed to open static pods config file {file_path:#?}: {e}"))?;
        match file.read_to_string(&mut content).await {
            Ok(_) => match serde_yaml::from_str(&content) {
                Ok(r) => res.push(r),
                Err(e) => warn!("Failed to parse pod config file {file_path:#?}: {e}. Skipped."),
            },
            Err(e) => warn!("Pod config file {file_path:#?} is not valid utf8: {e}, skipped."),
        }
    }
    Ok(res)
}
