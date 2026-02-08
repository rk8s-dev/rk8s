use anyhow::{Result, anyhow};
use common::{Deployment, RksMessage};
use std::fs::File;
use std::io::{self, Write};
use tabwriter::TabWriter;

use crate::commands::format_duration;
use crate::commands::pod::TLSConnectionArgs;
use crate::quic::client::{Cli, QUICClient};

/// Create a new Deployment
pub async fn create_deployment(
    deploy_yaml: &str,
    addr: &str,
    tls_cfg: TLSConnectionArgs,
) -> Result<()> {
    let cli = QUICClient::<Cli>::connect(addr, &tls_cfg).await?;
    println!("RKL connected to RKS at {addr}");

    let deploy = deployment_from_path(deploy_yaml)?;
    let deploy_name = deploy.metadata.name.clone();

    cli.send_msg(&RksMessage::CreateDeployment(deploy)).await?;

    match cli.fetch_msg().await? {
        RksMessage::Ack => {
            println!("deployment/{deploy_name} created");
            Ok(())
        }
        RksMessage::Error(err) => Err(anyhow!("Failed to create deployment: {}", err)),
        msg => Err(anyhow!("Unexpected response: {:?}", msg)),
    }
}

/// Apply (create or update) a Deployment
pub async fn apply_deployment(
    deploy_yaml: &str,
    addr: &str,
    tls_cfg: TLSConnectionArgs,
) -> Result<()> {
    let cli = QUICClient::<Cli>::connect(addr, &tls_cfg).await?;
    println!("RKL connected to RKS at {addr}");

    let deploy = deployment_from_path(deploy_yaml)?;
    let deploy_name = deploy.metadata.name.clone();

    cli.send_msg(&RksMessage::UpdateDeployment(deploy)).await?;

    match cli.fetch_msg().await? {
        RksMessage::Ack => {
            println!("deployment/{deploy_name} configured");
            Ok(())
        }
        RksMessage::Error(err) => Err(anyhow!("Failed to apply deployment: {}", err)),
        msg => Err(anyhow!("Unexpected response: {:?}", msg)),
    }
}

/// Delete a Deployment by name
pub async fn delete_deployment(
    deploy_name: &str,
    addr: &str,
    tls_cfg: TLSConnectionArgs,
) -> Result<()> {
    let cli = QUICClient::<Cli>::connect(addr, &tls_cfg).await?;
    println!("RKL connected to RKS at {addr}");

    cli.send_msg(&RksMessage::DeleteDeployment(deploy_name.to_string()))
        .await?;

    match cli.fetch_msg().await? {
        RksMessage::Ack => {
            println!("deployment/{deploy_name} deleted");
            Ok(())
        }
        RksMessage::Error(err) => Err(anyhow!("Failed to delete deployment: {}", err)),
        msg => Err(anyhow!("Unexpected response: {:?}", msg)),
    }
}

/// Get a specific Deployment
pub async fn get_deployment(
    deploy_name: &str,
    addr: &str,
    tls_cfg: TLSConnectionArgs,
) -> Result<()> {
    let cli = QUICClient::<Cli>::connect(addr, &tls_cfg).await?;
    println!("RKL connected to RKS at {addr}");

    cli.send_msg(&RksMessage::GetDeployment(deploy_name.to_string()))
        .await?;

    match cli.fetch_msg().await? {
        RksMessage::GetDeploymentRes(deploy) => {
            let yaml = serde_yaml::to_string(&*deploy)?;
            println!("{}", yaml);
            Ok(())
        }
        RksMessage::Error(err) => Err(anyhow!("Failed to get deployment: {}", err)),
        msg => Err(anyhow!("Unexpected response: {:?}", msg)),
    }
}

/// List all Deployments
pub async fn list_deployments(addr: &str, tls_cfg: TLSConnectionArgs) -> Result<()> {
    let cli = QUICClient::<Cli>::connect(addr, &tls_cfg).await?;
    println!("RKL connected to RKS at {addr}");

    cli.send_msg(&RksMessage::ListDeployment).await?;

    match cli.fetch_msg().await? {
        RksMessage::ListDeploymentRes(deps) => {
            list_print(deps)?;
            Ok(())
        }
        RksMessage::Error(err) => Err(anyhow!("Failed to list deployments: {}", err)),
        msg => Err(anyhow!("Unexpected response: {:?}", msg)),
    }
}

/// Rollback a Deployment to a specific revision
pub async fn rollback_deployment(
    deploy_name: &str,
    to_revision: i64,
    addr: &str,
    tls_cfg: TLSConnectionArgs,
) -> Result<()> {
    let cli = QUICClient::<Cli>::connect(addr, &tls_cfg).await?;
    println!("RKL connected to RKS at {addr}");

    cli.send_msg(&RksMessage::RollbackDeployment {
        name: deploy_name.to_string(),
        revision: to_revision,
    })
    .await?;

    match cli.fetch_msg().await? {
        RksMessage::Ack => {
            if to_revision == 0 {
                println!("deployment/{deploy_name} rolled back to previous revision");
            } else {
                println!("deployment/{deploy_name} rolled back to revision {to_revision}");
            }
            Ok(())
        }
        RksMessage::Error(err) => Err(anyhow!("Failed to rollback deployment: {}", err)),
        msg => Err(anyhow!("Unexpected response: {:?}", msg)),
    }
}

/// Get deployment revision history
pub async fn get_deployment_history(
    deploy_name: &str,
    addr: &str,
    tls_cfg: TLSConnectionArgs,
) -> Result<()> {
    let cli = QUICClient::<Cli>::connect(addr, &tls_cfg).await?;
    println!("RKL connected to RKS at {addr}");

    cli.send_msg(&RksMessage::GetDeploymentHistory(deploy_name.to_string()))
        .await?;

    match cli.fetch_msg().await? {
        RksMessage::DeploymentHistoryRes(history) => {
            history_print(deploy_name, history)?;
            Ok(())
        }
        RksMessage::Error(err) => Err(anyhow!("Failed to get deployment history: {}", err)),
        msg => Err(anyhow!("Unexpected response: {:?}", msg)),
    }
}

fn deployment_from_path(deploy_yaml: &str) -> Result<Box<Deployment>> {
    let deploy_file = File::open(deploy_yaml)
        .map_err(|e| anyhow!("Failed to open file '{}': {}", deploy_yaml, e))?;
    let deploy: Deployment =
        serde_yaml::from_reader(deploy_file).map_err(|e| anyhow!("Failed to parse YAML: {}", e))?;

    // Validate the Deployment
    validate_deployment(&deploy)?;

    Ok(Box::new(deploy))
}

fn validate_deployment(deploy: &Deployment) -> Result<()> {
    use common::LabelSelectorOperator;

    // Name must not be empty
    if deploy.metadata.name.is_empty() {
        return Err(anyhow!("Deployment metadata.name must not be empty"));
    }

    // Replicas must be non-negative
    if deploy.spec.replicas < 0 {
        return Err(anyhow!(
            "Deployment spec.replicas must be non-negative, got {}",
            deploy.spec.replicas
        ));
    }

    // Selector must not be empty
    if deploy.spec.selector.match_labels.is_empty()
        && deploy.spec.selector.match_expressions.is_empty()
    {
        return Err(anyhow!(
            "Deployment spec.selector must have at least one matchLabel or matchExpression"
        ));
    }

    // Template must have at least one container
    if deploy.spec.template.spec.containers.is_empty() {
        return Err(anyhow!(
            "Deployment spec.template.spec.containers must not be empty"
        ));
    }

    let template_labels = &deploy.spec.template.metadata.labels;

    // Selector matchLabels must be a subset of template labels
    for (key, value) in &deploy.spec.selector.match_labels {
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
    for expr in &deploy.spec.selector.match_expressions {
        match &expr.operator {
            LabelSelectorOperator::In => match template_labels.get(&expr.key) {
                Some(value) if expr.values.contains(value) => {}
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
            },
            LabelSelectorOperator::NotIn => {
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
            }
            LabelSelectorOperator::Exists => {
                if !template_labels.contains_key(&expr.key) {
                    return Err(anyhow!(
                        "Selector matchExpressions: template missing required label '{}' (Exists operator)",
                        expr.key
                    ));
                }
            }
            LabelSelectorOperator::DoesNotExist => {
                if template_labels.contains_key(&expr.key) {
                    return Err(anyhow!(
                        "Selector matchExpressions: template has forbidden label '{}' (DoesNotExist operator)",
                        expr.key
                    ));
                }
            }
        }
    }

    // Validate rolling update strategy parameters
    if let common::DeploymentStrategy::RollingUpdate { rolling_update } = &deploy.spec.strategy {
        // Validate maxSurge
        match &rolling_update.max_surge {
            common::IntOrPercentage::Int(n) if *n < 0 => {
                return Err(anyhow!(
                    "RollingUpdate strategy maxSurge must be non-negative, got {}",
                    n
                ));
            }
            common::IntOrPercentage::String(s) => {
                if let Some(percent_str) = s.strip_suffix('%') {
                    if percent_str.parse::<f64>().map(|p| p < 0.0).unwrap_or(true) {
                        return Err(anyhow!(
                            "RollingUpdate strategy maxSurge percentage is invalid: {}",
                            s
                        ));
                    }
                } else if s.parse::<i32>().map(|n| n < 0).unwrap_or(true) {
                    return Err(anyhow!("RollingUpdate strategy maxSurge is invalid: {}", s));
                }
            }
            _ => {}
        }

        // Validate maxUnavailable
        match &rolling_update.max_unavailable {
            common::IntOrPercentage::Int(n) if *n < 0 => {
                return Err(anyhow!(
                    "RollingUpdate strategy maxUnavailable must be non-negative, got {}",
                    n
                ));
            }
            common::IntOrPercentage::String(s) => {
                if let Some(percent_str) = s.strip_suffix('%') {
                    if percent_str.parse::<f64>().map(|p| p < 0.0).unwrap_or(true) {
                        return Err(anyhow!(
                            "RollingUpdate strategy maxUnavailable percentage is invalid: {}",
                            s
                        ));
                    }
                } else if s.parse::<i32>().map(|n| n < 0).unwrap_or(true) {
                    return Err(anyhow!(
                        "RollingUpdate strategy maxUnavailable is invalid: {}",
                        s
                    ));
                }
            }
            _ => {}
        }

        // maxSurge and maxUnavailable cannot both be 0
        let surge_zero = match &rolling_update.max_surge {
            common::IntOrPercentage::Int(0) => true,
            common::IntOrPercentage::String(s) if s == "0" || s == "0%" => true,
            _ => false,
        };
        let unavailable_zero = match &rolling_update.max_unavailable {
            common::IntOrPercentage::Int(0) => true,
            common::IntOrPercentage::String(s) if s == "0" || s == "0%" => true,
            _ => false,
        };
        if surge_zero && unavailable_zero {
            return Err(anyhow!(
                "RollingUpdate strategy: maxSurge and maxUnavailable cannot both be 0"
            ));
        }
    }

    // Validate revisionHistoryLimit
    if deploy.spec.revision_history_limit < 0 {
        return Err(anyhow!(
            "Deployment spec.revisionHistoryLimit must be non-negative, got {}",
            deploy.spec.revision_history_limit
        ));
    }

    // Validate progressDeadlineSeconds
    if deploy.spec.progress_deadline_seconds < 0 {
        return Err(anyhow!(
            "Deployment spec.progressDeadlineSeconds must be non-negative, got {}",
            deploy.spec.progress_deadline_seconds
        ));
    }

    Ok(())
}

/// Print deployments in a table format
fn list_print(deps: Vec<Deployment>) -> Result<()> {
    let mut tw = TabWriter::new(io::stdout());
    writeln!(tw, "NAME\tREADY\tUP-TO-DATE\tAVAILABLE\tAGE\tSTRATEGY")?;

    for dep in deps {
        let name = &dep.metadata.name;
        let replicas = dep.spec.replicas;
        let ready = dep.status.ready_replicas;
        let updated = dep.status.updated_replicas;
        let available = dep.status.available_replicas;

        // Calculate age
        let age = if let Some(ts) = dep.metadata.creation_timestamp {
            let now = chrono::Utc::now();
            let duration = now.signed_duration_since(ts);
            format_duration(duration)
        } else {
            "<unknown>".to_string()
        };

        let strategy = match &dep.spec.strategy {
            common::DeploymentStrategy::RollingUpdate { .. } => "RollingUpdate",
            common::DeploymentStrategy::Recreate => "Recreate",
        };

        writeln!(
            tw,
            "{}\t{}/{}\t{}\t{}\t{}\t{}",
            name, ready, replicas, updated, available, age, strategy
        )?;
    }

    tw.flush()?;
    Ok(())
}

/// Print deployment revision history in a table format
fn history_print(deploy_name: &str, history: Vec<common::DeploymentRevisionInfo>) -> Result<()> {
    println!("deployment/{deploy_name}");

    let mut tw = TabWriter::new(io::stdout());
    writeln!(tw, "REVISION\tREPLICASET\tIMAGE\tREPLICAS\tCURRENT")?;

    for info in history {
        let current_marker = if info.is_current { "*" } else { "" };
        let image = info.image.unwrap_or_else(|| "<none>".to_string());
        writeln!(
            tw,
            "{}{}\t{}\t{}\t{}\t{}",
            info.revision,
            current_marker,
            info.replicaset_name,
            image,
            info.replicas,
            if info.is_current { "yes" } else { "" }
        )?;
    }

    tw.flush()?;
    Ok(())
}
