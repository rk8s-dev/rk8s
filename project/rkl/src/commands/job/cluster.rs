use anyhow::{Result, anyhow};
use common::{Job, RksMessage};
use std::fs::File;
use std::io::{self, Write};
use tabwriter::TabWriter;

use crate::commands::format_duration;
use crate::commands::pod::TLSConnectionArgs;
use crate::quic::client::{Cli, QUICClient};

/// Create a new Job
pub async fn create_job(job_yaml: &str, addr: &str, tls_cfg: TLSConnectionArgs) -> Result<()> {
    let cli = QUICClient::<Cli>::connect(addr, &tls_cfg).await?;
    println!("RKL connected to RKS at {addr}");

    let job = job_from_path(job_yaml)?;
    let job_name = job.metadata.name.clone();

    cli.send_msg(&RksMessage::CreateJob(job)).await?;

    match cli.fetch_msg().await? {
        RksMessage::Ack => {
            println!("job/{job_name} created");
            Ok(())
        }
        RksMessage::Error(err) => Err(anyhow!("Failed to create job: {}", err)),
        msg => Err(anyhow!("Unexpected response: {:?}", msg)),
    }
}

/// Apply (create or update) a Job
pub async fn apply_job(job_yaml: &str, addr: &str, tls_cfg: TLSConnectionArgs) -> Result<()> {
    let cli = QUICClient::<Cli>::connect(addr, &tls_cfg).await?;
    println!("RKL connected to RKS at {addr}");

    let job = job_from_path(job_yaml)?;
    let job_name = job.metadata.name.clone();

    cli.send_msg(&RksMessage::UpdateJob(job)).await?;

    match cli.fetch_msg().await? {
        RksMessage::Ack => {
            println!("job/{job_name} configured");
            Ok(())
        }
        RksMessage::Error(err) => Err(anyhow!("Failed to apply job: {}", err)),
        msg => Err(anyhow!("Unexpected response: {:?}", msg)),
    }
}

/// Delete a Job by name
pub async fn delete_job(job_name: &str, addr: &str, tls_cfg: TLSConnectionArgs) -> Result<()> {
    let cli = QUICClient::<Cli>::connect(addr, &tls_cfg).await?;
    println!("RKL connected to RKS at {addr}");

    cli.send_msg(&RksMessage::DeleteJob(job_name.to_string()))
        .await?;

    match cli.fetch_msg().await? {
        RksMessage::Ack => {
            println!("job/{job_name} deleted");
            Ok(())
        }
        RksMessage::Error(err) => Err(anyhow!("Failed to delete job: {}", err)),
        msg => Err(anyhow!("Unexpected response: {:?}", msg)),
    }
}

/// Get a specific Job
pub async fn get_job(job_name: &str, addr: &str, tls_cfg: TLSConnectionArgs) -> Result<()> {
    let cli = QUICClient::<Cli>::connect(addr, &tls_cfg).await?;
    println!("RKL connected to RKS at {addr}");

    cli.send_msg(&RksMessage::GetJob(job_name.to_string()))
        .await?;

    match cli.fetch_msg().await? {
        RksMessage::GetJobRes(job) => {
            let yaml = serde_yaml::to_string(&*job)?;
            println!("{}", yaml);
            Ok(())
        }
        RksMessage::Error(err) => Err(anyhow!("Failed to get job: {}", err)),
        msg => Err(anyhow!("Unexpected response: {:?}", msg)),
    }
}

/// List all Jobs
pub async fn list_jobs(addr: &str, tls_cfg: TLSConnectionArgs) -> Result<()> {
    let cli = QUICClient::<Cli>::connect(addr, &tls_cfg).await?;
    println!("RKL connected to RKS at {addr}");

    cli.send_msg(&RksMessage::ListJob).await?;

    match cli.fetch_msg().await? {
        RksMessage::ListJobRes(jobs) => {
            list_print(jobs)?;
            Ok(())
        }
        RksMessage::Error(err) => Err(anyhow!("Failed to list jobs: {}", err)),
        msg => Err(anyhow!("Unexpected response: {:?}", msg)),
    }
}

fn job_from_path(job_yaml: &str) -> Result<Box<Job>> {
    let f =
        File::open(job_yaml).map_err(|e| anyhow!("Failed to open file '{}': {}", job_yaml, e))?;
    let job: Job =
        serde_yaml::from_reader(f).map_err(|e| anyhow!("Failed to parse YAML: {}", e))?;

    validate_job(&job)?;

    Ok(Box::new(job))
}

fn validate_job(job: &Job) -> Result<()> {
    if job.metadata.name.is_empty() {
        return Err(anyhow!("Job metadata.name must not be empty"));
    }

    if job.spec.completions < 0 {
        return Err(anyhow!(
            "Job spec.completions must be non-negative, got {}",
            job.spec.completions
        ));
    }
    if job.spec.parallelism < 0 {
        return Err(anyhow!(
            "Job spec.parallelism must be non-negative, got {}",
            job.spec.parallelism
        ));
    }

    if job.spec.template.spec.containers.is_empty() {
        return Err(anyhow!(
            "Job spec.template must include at least one container"
        ));
    }

    Ok(())
}

fn list_print(jobs: Vec<Job>) -> Result<()> {
    let mut tab_writer = TabWriter::new(io::stdout());
    writeln!(
        &mut tab_writer,
        "NAME\tCOMPLETIONS\tSUCCESSFUL\tFAILED\tACTIVE\tAGE"
    )?;

    for job in jobs {
        let name = &job.metadata.name;
        let want = job.spec.completions.max(0);
        let succ = job.status.succeeded;
        let fail = job.status.failed;
        let active = job.status.active;
        let completions = format!("{}/{}", succ, want);

        let age = job
            .metadata
            .creation_timestamp
            .map(|ts| {
                let now = chrono::Utc::now();
                let duration = now.signed_duration_since(ts);
                format_duration(duration)
            })
            .unwrap_or_else(|| "<unknown>".to_string());

        writeln!(
            &mut tab_writer,
            "{}\t{}\t{}\t{}\t{}\t{}",
            name, completions, succ, fail, active, age
        )?;
    }

    tab_writer.flush()?;
    Ok(())
}
