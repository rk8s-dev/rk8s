use anyhow::Ok;
use anyhow::Result;
use anyhow::anyhow;
use chrono::Utc;
use common::PodTask;
use common::RksMessage;
use std::fs::File;
use std::io;
use std::io::Write;
use tabwriter::TabWriter;
use tracing::info;

use crate::commands::format_duration;
use crate::commands::pod::TLSConnectionArgs;
use crate::quic::client::{Cli, QUICClient};

pub async fn delete_pod(pod_name: &str, addr: &str, tls_cfg: TLSConnectionArgs) -> Result<()> {
    let cli = QUICClient::<Cli>::connect(addr, &tls_cfg).await?;
    info!("RKL connected to RKS at {addr}");

    cli.send_msg(&RksMessage::DeletePod(pod_name.to_string()))
        .await?;
    let _ = cli.fetch_msg().await?;
    info!("pod {pod_name} deleted");
    Ok(())
}

pub async fn create_pod(pod_yaml: &str, addr: &str, tls_cfg: TLSConnectionArgs) -> Result<()> {
    let cli = QUICClient::<Cli>::connect(addr, &tls_cfg).await.unwrap();
    info!("RKL connected to RKS at {addr}");

    let task = pod_task_from_path(pod_yaml).map_err(|e| anyhow!("invalid pod yaml: {}", e))?;
    let pod_name = task.metadata.name.clone();
    cli.send_msg(&RksMessage::CreatePod(task)).await?;
    let _ = cli.fetch_msg().await?;
    info!("pod {pod_name} created");
    Ok(())
}

pub async fn list_pod(addr: &str, tls_cfg: TLSConnectionArgs) -> Result<()> {
    let cli = QUICClient::<Cli>::connect(addr, &tls_cfg).await?;
    info!("RKL connected to RKS at {addr}");

    cli.send_msg(&RksMessage::ListPod).await?;

    match cli.fetch_msg().await? {
        RksMessage::ListPodRes(res) => list_print(res),
        msg => Err(anyhow!("unexpected response {:?} ", msg)),
    }
}

pub async fn get_pod(pod_name: &str, rks_addr: &str, tls_cfg: TLSConnectionArgs) -> Result<()> {
    let cli = QUICClient::<Cli>::connect(rks_addr, &tls_cfg).await?;
    info!("RKL connected to RKS at {rks_addr}");

    cli.send_msg(&RksMessage::GetPod(pod_name.to_string()))
        .await?;

    match cli.fetch_msg().await? {
        RksMessage::GetPodRes(pod) => {
            let yaml = serde_yaml::to_string(&*pod)?;
            println!("{}", yaml);
            Ok(())
        }
        RksMessage::Error(err) => Err(anyhow!("Failed to get pod: {}", err)),
        msg => Err(anyhow!("unexpected response {:?} ", msg)),
    }
}

pub fn pod_task_from_path(pod_yaml: &str) -> Result<Box<PodTask>> {
    let pod_file = File::open(pod_yaml)?;
    let task: PodTask = serde_yaml::from_reader(pod_file)?;
    Ok(Box::new(task))
}

fn list_print(pod_list: Vec<PodTask>) -> Result<()> {
    let mut tab_writer = TabWriter::new(io::stdout());
    writeln!(&mut tab_writer, "NAME\tREADY\tSTATUS\tRESTARTS\tAGE")?;
    for pod in &pod_list {
        let name = &pod.metadata.name;
        let ready_count = pod
            .status
            .container_statuses
            .iter()
            .filter(|c| c.ready)
            .count();
        let total_count = pod.spec.containers.len();
        let ready = format!("{}/{}", ready_count, total_count);
        let status = format!("{:?}", pod.status.phase);
        let restarts: u32 = pod
            .status
            .container_statuses
            .iter()
            .map(|c| c.restart_count)
            .sum();
        let age = pod
            .metadata
            .creation_timestamp
            .map(|ts| {
                let duration = Utc::now().signed_duration_since(ts);
                format_duration(duration)
            })
            .unwrap_or_else(|| "<unknown>".into());
        writeln!(
            &mut tab_writer,
            "{}\t{}\t{}\t{}\t{}",
            name, ready, status, restarts, age
        )?;
    }
    tab_writer.flush()?;
    Ok(())
}
