use anyhow::{Result, anyhow};
use common::{ReplicaSet, RksMessage};
use std::fs::File;
use std::io::{self, Write};
use tabwriter::TabWriter;

use crate::commands::pod::TLSConnectionArgs;
use crate::quic::client::{Cli, QUICClient};

/// Create a new ReplicaSet
pub async fn create_replicaset(
    rs_yaml: &str,
    addr: &str,
    tls_cfg: TLSConnectionArgs,
) -> Result<()> {
    let cli = QUICClient::<Cli>::connect(addr, &tls_cfg).await?;
    println!("RKL connected to RKS at {addr}");

    let rs = replicaset_from_path(rs_yaml)?;
    let rs_name = rs.metadata.name.clone();

    cli.send_msg(&RksMessage::CreateReplicaSet(rs)).await?;

    match cli.fetch_msg().await? {
        RksMessage::Ack => {
            println!("replicaset {rs_name} created");
            Ok(())
        }
        RksMessage::Error(err) => Err(anyhow!("Failed to create replicaset: {}", err)),
        msg => Err(anyhow!("Unexpected response: {:?}", msg)),
    }
}

/// Apply (create or update) a ReplicaSet
pub async fn apply_replicaset(rs_yaml: &str, addr: &str, tls_cfg: TLSConnectionArgs) -> Result<()> {
    let cli = QUICClient::<Cli>::connect(addr, &tls_cfg).await?;
    println!("RKL connected to RKS at {addr}");

    let rs = replicaset_from_path(rs_yaml)?;
    let rs_name = rs.metadata.name.clone();

    cli.send_msg(&RksMessage::UpdateReplicaSet(rs)).await?;

    match cli.fetch_msg().await? {
        RksMessage::Ack => {
            println!("replicaset {rs_name} applied");
            Ok(())
        }
        RksMessage::Error(err) => Err(anyhow!("Failed to apply replicaset: {}", err)),
        msg => Err(anyhow!("Unexpected response: {:?}", msg)),
    }
}

/// Delete a ReplicaSet by name
pub async fn delete_replicaset(
    rs_name: &str,
    addr: &str,
    tls_cfg: TLSConnectionArgs,
) -> Result<()> {
    let cli = QUICClient::<Cli>::connect(addr, &tls_cfg).await?;
    println!("RKL connected to RKS at {addr}");

    cli.send_msg(&RksMessage::DeleteReplicaSet(rs_name.to_string()))
        .await?;

    match cli.fetch_msg().await? {
        RksMessage::Ack => {
            println!("replicaset {rs_name} deleted");
            Ok(())
        }
        RksMessage::Error(err) => Err(anyhow!("Failed to delete replicaset: {}", err)),
        msg => Err(anyhow!("Unexpected response: {:?}", msg)),
    }
}

/// Get a specific ReplicaSet
pub async fn get_replicaset(rs_name: &str, addr: &str, tls_cfg: TLSConnectionArgs) -> Result<()> {
    let cli = QUICClient::<Cli>::connect(addr, &tls_cfg).await?;
    println!("RKL connected to RKS at {addr}");

    cli.send_msg(&RksMessage::GetReplicaSet(rs_name.to_string()))
        .await?;

    match cli.fetch_msg().await? {
        RksMessage::GetReplicaSetRes(rs) => {
            let yaml = serde_yaml::to_string(&*rs)?;
            println!("{}", yaml);
            Ok(())
        }
        RksMessage::Error(err) => Err(anyhow!("Failed to get replicaset: {}", err)),
        msg => Err(anyhow!("Unexpected response: {:?}", msg)),
    }
}

/// List all ReplicaSets
pub async fn list_replicasets(addr: &str, tls_cfg: TLSConnectionArgs) -> Result<()> {
    let cli = QUICClient::<Cli>::connect(addr, &tls_cfg).await?;
    println!("RKL connected to RKS at {addr}");

    cli.send_msg(&RksMessage::ListReplicaSet).await?;

    match cli.fetch_msg().await? {
        RksMessage::ListReplicaSetRes(rss) => {
            list_print(rss)?;
            Ok(())
        }
        msg => Err(anyhow!("Unexpected response: {:?}", msg)),
    }
}

fn replicaset_from_path(rs_yaml: &str) -> Result<Box<ReplicaSet>> {
    let rs_file = File::open(rs_yaml)?;
    let rs: ReplicaSet = serde_yaml::from_reader(rs_file)?;

    // Validate the ReplicaSet
    validate_replicaset(&rs)?;

    Ok(Box::new(rs))
}

fn validate_replicaset(rs: &ReplicaSet) -> Result<()> {
    use common::LabelSelectorOperator;

    // Replicas must be non-negative
    if rs.spec.replicas < 0 {
        return Err(anyhow!(
            "ReplicaSet spec.replicas must be non-negative, got {}",
            rs.spec.replicas
        ));
    }

    let template_labels = &rs.spec.template.metadata.labels;

    // Selector matchLabels must be a subset of template labels
    for (key, value) in &rs.spec.selector.match_labels {
        match template_labels.get(key) {
            Some(template_value) if template_value == value => {}
            Some(template_value) => {
                return Err(anyhow!(
                    "Selector matchLabels '{}={}' does not match template label '{}={}'",
                    key,
                    value,
                    key,
                    template_value
                ));
            }
            None => {
                return Err(anyhow!(
                    "Selector matchLabels '{}={}' not found in pod template labels",
                    key,
                    value
                ));
            }
        }
    }

    // Validate matchExpressions against template labels
    for expr in &rs.spec.selector.match_expressions {
        match &expr.operator {
            LabelSelectorOperator::In => {
                // Template must have this key with a value in the specified set
                match template_labels.get(&expr.key) {
                    Some(value) if expr.values.contains(value) => {
                        // OK
                    }
                    Some(value) => {
                        return Err(anyhow!(
                            "Selector matchExpressions: template label '{}={}' value not in required set {:?}",
                            expr.key,
                            value,
                            expr.values
                        ));
                    }
                    None => {
                        return Err(anyhow!(
                            "Selector matchExpressions: template missing required label '{}' (In operator)",
                            expr.key
                        ));
                    }
                }
            }
            LabelSelectorOperator::NotIn => {
                // If template has this key, its value must not be in the specified set
                if let Some(value) = template_labels.get(&expr.key)
                    && expr.values.contains(value)
                {
                    return Err(anyhow!(
                        "Selector matchExpressions: template label '{}={}' is in forbidden set {:?} (NotIn operator)",
                        expr.key,
                        value,
                        expr.values
                    ));
                }

                // OK if key doesn't exist or value not in set
            }
            LabelSelectorOperator::Exists => {
                // Template must have this key (any value is ok)
                if !template_labels.contains_key(&expr.key) {
                    return Err(anyhow!(
                        "Selector matchExpressions: template missing required label '{}' (Exists operator)",
                        expr.key
                    ));
                }
            }
            LabelSelectorOperator::DoesNotExist => {
                // Template must not have this key
                if template_labels.contains_key(&expr.key) {
                    return Err(anyhow!(
                        "Selector matchExpressions: template has forbidden label '{}' (DoesNotExist operator)",
                        expr.key
                    ));
                }
            }
        }
    }

    Ok(())
}

fn list_print(rs_list: Vec<ReplicaSet>) -> Result<()> {
    let mut tab_writer = TabWriter::new(io::stdout());
    writeln!(&mut tab_writer, "NAME\tDESIRED\tCURRENT\tREADY\tAGE")?;

    for rs in rs_list {
        let name = &rs.metadata.name;
        let desired = rs.spec.replicas;
        let current = rs.status.replicas;
        let ready = rs.status.ready_replicas;

        let age = rs
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
            "{}\t{}\t{}\t{}\t{}",
            name, desired, current, ready, age
        )?;
    }

    tab_writer.flush()?;
    Ok(())
}

fn format_duration(duration: chrono::Duration) -> String {
    let days = duration.num_days();
    let hours = duration.num_hours() % 24;
    let minutes = duration.num_minutes() % 60;
    let seconds = duration.num_seconds() % 60;

    if days > 0 {
        format!("{}d{}h", days, hours)
    } else if hours > 0 {
        format!("{}h{}m", hours, minutes)
    } else if minutes > 0 {
        format!("{}m{}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}
