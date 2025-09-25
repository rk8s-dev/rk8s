use futures::StreamExt;
use futures::stream::FuturesUnordered;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use oci_client::client::PushResponse;
use oci_client::errors::OciDistributionError;
use oci_spec::distribution::ErrorResponse;
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

        let mut has_error = false;
        while let Some((result, digest)) = stream.next().await {
            let pb = bars[&digest].clone();
            match result {
                Ok(_) => pb.finish_with_message("pushed"),
                Err(e) => {
                    pb.finish_with_message(format!("failed: {}", format_push_error(&e)?));
                    has_error = true;
                }
            }
        }

        if !has_error {
            Ok(())
        } else {
            anyhow::bail!("could not to push due to previous error")
        }
    }
}

fn format_push_error(e: &OciDistributionError) -> anyhow::Result<String> {
    match e {
        OciDistributionError::ServerError { message, .. } => {
            let errors = serde_json::from_str::<ErrorResponse>(message)?;
            let first_error = &errors.detail()[0];
            first_error
                .message()
                .as_ref()
                .map(|e| e.to_string())
                .ok_or_else(|| {
                    anyhow::anyhow!("response from distribution should include error message")
                })
        }
        _ => Ok(e.to_string()),
    }
}
