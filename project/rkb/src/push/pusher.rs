use futures::StreamExt;
use futures::stream::FuturesUnordered;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use oci_client::client::PushResponse;
use oci_client::errors::OciDistributionError;
use std::collections::HashMap;
use std::pin::Pin;
use std::time::Duration;

type BoxedFuture = Pin<Box<dyn Future<Output = Result<PushResponse, OciDistributionError>> + Send>>;

pub struct PushTask {
    digest: String,
    task: BoxedFuture,
}

impl PushTask {
    pub fn new(digest: impl Into<String>, task: BoxedFuture) -> Self {
        PushTask {
            digest: digest.into(),
            task,
        }
    }
}

pub struct Pusher {
    tasks: Vec<PushTask>,
    progress: MultiProgress,
}

impl Pusher {
    pub fn new(tasks: Vec<PushTask>) -> Self {
        Self {
            tasks,
            progress: MultiProgress::new(),
        }
    }

    pub async fn push_all(self) -> anyhow::Result<()> {
        let ticking_style = ProgressStyle::default_spinner()
            .template("{spinner:.green} {prefix:12.cyan} {msg}")
            .expect("Failed to create style");

        let mut bars = HashMap::new();
        for task in &self.tasks {
            let pb = self.progress.add(ProgressBar::new_spinner());
            pb.set_style(ticking_style.clone());
            pb.set_prefix(task.digest[..12.min(task.digest.len())].to_string());
            pb.set_message("pushing");
            pb.enable_steady_tick(Duration::from_millis(100));
            bars.insert(task.digest.to_string(), pb);
        }

        let mut stream = self
            .tasks
            .into_iter()
            .map(|e| async move { (e.task.await, e.digest) })
            .collect::<FuturesUnordered<_>>();

        while let Some((result, digest)) = stream.next().await {
            let pb = bars[&digest].clone();
            match result {
                Ok(_) => pb.finish_with_message("pushed"),
                Err(e) => pb.finish_with_message(format!("failed: {e}")),
            }
        }
        Ok(())
    }
}
