use std::sync::Arc;

use crate::api::xlinestore::XlineStore;
use anyhow::Result;
use common::PodTask;
use libscheduler::{
    models::Assignment,
    plugins::{Plugins, node_resources_fit::ScoringStrategy},
    with_xline::run_scheduler_with_xline,
};
use libvault::storage::xline::XlineOptions;
use log::{debug, error};
use tokio::sync::mpsc;

pub struct Scheduler {
    assignment_rx: mpsc::UnboundedReceiver<Result<Assignment, anyhow::Error>>,
    xline_store: Arc<XlineStore>,
}

impl Scheduler {
    pub async fn try_new(
        xline_options: XlineOptions,
        xline_store: Arc<XlineStore>,
        scoring_strategy: ScoringStrategy,
        plugins: Plugins,
    ) -> Result<Self> {
        let (_unassume_tx, unassume_rx) = mpsc::unbounded_channel();
        let assignment_rx =
            run_scheduler_with_xline(xline_options, scoring_strategy, plugins, unassume_rx).await?;
        Ok(Self {
            assignment_rx,
            xline_store,
        })
    }

    /// Runs the scheduler's main loop to process pod assignments.
    ///
    /// Spawns a background task that continuously:
    /// - Receives pod assignments from the scheduler
    /// - Updates the pod's node assignment in the xline store
    ///
    /// Returns immediately after spawning the background task.
    pub async fn run(mut self) {
        debug!("Scheduler is running");
        tokio::spawn(async move {
            loop {
                // if get an assignment from the scheduler, then modify the pod spec 's node_name and save to xline store
                if let Some(Ok(assignment)) = self.assignment_rx.recv().await
                    && let Ok(Some(pod_yaml)) =
                        self.xline_store.get_pod_yaml(&assignment.pod_name).await
                {
                    debug!(
                        "Received assignment for pod {}: node {}",
                        assignment.pod_name, assignment.node_name
                    );
                    let yaml =
                        serde_yaml::from_str::<PodTask>(&pod_yaml).and_then(|mut pod_task| {
                            pod_task.spec.node_name = Some(assignment.node_name);
                            serde_yaml::to_string(&pod_task)
                        });

                    if let Ok(yaml_string) = yaml {
                        debug!(
                            "Updating pod {} with new node assignment in xline store",
                            assignment.pod_name
                        );
                        if let Err(e) = self
                            .xline_store
                            .insert_pod_yaml(&assignment.pod_name, &yaml_string)
                            .await
                        {
                            error!(
                                "Failed to update pod {} in xline store: {e:?}",
                                assignment.pod_name
                            );
                        }
                    }
                }
            }
        });
    }
}
