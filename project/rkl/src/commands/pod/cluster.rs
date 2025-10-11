use anyhow::Ok;
use anyhow::Result;
use anyhow::anyhow;
use common::PodTask;
use common::RksMessage;
use std::fs::File;
use std::io;
use std::io::Write;
use tabwriter::TabWriter;
use tracing::info;

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

pub fn pod_task_from_path(pod_yaml: &str) -> Result<Box<PodTask>> {
    let pod_file = File::open(pod_yaml)?;
    let task: PodTask = serde_yaml::from_reader(pod_file)?;
    Ok(Box::new(task))
}

fn list_print(pod_list: Vec<String>) -> Result<()> {
    let mut tab_writer = TabWriter::new(io::stdout());
    writeln!(&mut tab_writer, "NAME\tREADY\tSTATUS\tRESTARTS\tAGE")?;
    for pod in pod_list {
        let _ = writeln!(&mut tab_writer, "{pod}");
    }
    tab_writer.flush()?;
    Ok(())
}
