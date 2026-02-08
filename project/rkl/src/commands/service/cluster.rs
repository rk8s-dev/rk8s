use anyhow::{Result, anyhow};
use common::{LabelSelectorOperator, RksMessage, ServicePort, ServiceTask};
use std::fs::File;
use std::io::{self, Write};
use tabwriter::TabWriter;

use crate::commands::pod::TLSConnectionArgs;
use crate::quic::client::{Cli, QUICClient};

/// Create a new Service
pub async fn create_service(svc_yaml: &str, addr: &str, tls_cfg: TLSConnectionArgs) -> Result<()> {
    let cli = QUICClient::<Cli>::connect(addr, &tls_cfg).await?;
    println!("RKL connected to RKS at {addr}");

    let svc = service_from_path(svc_yaml)?;
    let svc_name = svc.metadata.name.clone();

    cli.send_msg(&RksMessage::CreateService(svc)).await?;

    match cli.fetch_msg().await? {
        RksMessage::Ack => {
            println!("service/{svc_name} created");
            Ok(())
        }
        RksMessage::Error(err) => Err(anyhow!("Failed to create service: {}", err)),
        msg => Err(anyhow!("Unexpected response: {:?}", msg)),
    }
}

/// Apply (create or update) a Service
pub async fn apply_service(svc_yaml: &str, addr: &str, tls_cfg: TLSConnectionArgs) -> Result<()> {
    let cli = QUICClient::<Cli>::connect(addr, &tls_cfg).await?;
    println!("RKL connected to RKS at {addr}");

    let svc = service_from_path(svc_yaml)?;
    let svc_name = svc.metadata.name.clone();

    cli.send_msg(&RksMessage::UpdateService(svc)).await?;

    match cli.fetch_msg().await? {
        RksMessage::Ack => {
            println!("service/{svc_name} configured");
            Ok(())
        }
        RksMessage::Error(err) => Err(anyhow!("Failed to apply service: {}", err)),
        msg => Err(anyhow!("Unexpected response: {:?}", msg)),
    }
}

/// Delete a Service by name
pub async fn delete_service(svc_name: &str, addr: &str, tls_cfg: TLSConnectionArgs) -> Result<()> {
    let cli = QUICClient::<Cli>::connect(addr, &tls_cfg).await?;
    println!("RKL connected to RKS at {addr}");

    cli.send_msg(&RksMessage::DeleteService(svc_name.to_string()))
        .await?;

    match cli.fetch_msg().await? {
        RksMessage::Ack => {
            println!("service/{svc_name} deleted");
            Ok(())
        }
        RksMessage::Error(err) => Err(anyhow!("Failed to delete service: {}", err)),
        msg => Err(anyhow!("Unexpected response: {:?}", msg)),
    }
}

/// Get a specific Service
pub async fn get_service(svc_name: &str, addr: &str, tls_cfg: TLSConnectionArgs) -> Result<()> {
    let cli = QUICClient::<Cli>::connect(addr, &tls_cfg).await?;
    println!("RKL connected to RKS at {addr}");

    cli.send_msg(&RksMessage::GetService(svc_name.to_string()))
        .await?;

    match cli.fetch_msg().await? {
        RksMessage::GetServiceRes(svc) => {
            let yaml = serde_yaml::to_string(&*svc)?;
            println!("{}", yaml);
            Ok(())
        }
        RksMessage::Error(err) => Err(anyhow!("Failed to get service: {}", err)),
        msg => Err(anyhow!("Unexpected response: {:?}", msg)),
    }
}

/// List all Services
pub async fn list_services(addr: &str, tls_cfg: TLSConnectionArgs) -> Result<()> {
    let cli = QUICClient::<Cli>::connect(addr, &tls_cfg).await?;
    println!("RKL connected to RKS at {addr}");

    cli.send_msg(&RksMessage::ListService).await?;

    match cli.fetch_msg().await? {
        RksMessage::ListServiceRes(services) => {
            list_print(services)?;
            Ok(())
        }
        RksMessage::Error(err) => Err(anyhow!("Failed to list services: {}", err)),
        msg => Err(anyhow!("Unexpected response: {:?}", msg)),
    }
}

fn service_from_path(svc_yaml: &str) -> Result<Box<ServiceTask>> {
    let svc_file =
        File::open(svc_yaml).map_err(|e| anyhow!("Failed to open file '{}': {}", svc_yaml, e))?;
    let svc: ServiceTask =
        serde_yaml::from_reader(svc_file).map_err(|e| anyhow!("Failed to parse YAML: {}", e))?;

    validate_service(&svc)?;

    Ok(Box::new(svc))
}

fn validate_service(svc: &ServiceTask) -> Result<()> {
    if svc.metadata.name.is_empty() {
        return Err(anyhow!("Service metadata.name must not be empty"));
    }

    if svc.spec.service_type.trim().is_empty() {
        return Err(anyhow!("Service spec.type must not be empty"));
    }

    if let Some(selector) = &svc.spec.selector {
        if selector.match_labels.is_empty() && selector.match_expressions.is_empty() {
            return Err(anyhow!(
                "Service spec.selector must have at least one matchLabel or matchExpression"
            ));
        }

        for expr in &selector.match_expressions {
            match expr.operator {
                LabelSelectorOperator::In | LabelSelectorOperator::NotIn => {
                    if expr.values.is_empty() {
                        return Err(anyhow!(
                            "Service selector matchExpressions for '{}' requires values",
                            expr.key
                        ));
                    }
                }
                LabelSelectorOperator::Exists | LabelSelectorOperator::DoesNotExist => {}
            }
        }
    }

    for port in &svc.spec.ports {
        if port.port <= 0 {
            return Err(anyhow!("Service port must be positive, got {}", port.port));
        }
        if let Some(target_port) = port.target_port
            && target_port <= 0
        {
            return Err(anyhow!(
                "Service targetPort must be positive, got {}",
                target_port
            ));
        }
        if let Some(node_port) = port.node_port
            && node_port <= 0
        {
            return Err(anyhow!(
                "Service nodePort must be positive, got {}",
                node_port
            ));
        }
    }

    Ok(())
}

fn list_print(services: Vec<ServiceTask>) -> Result<()> {
    let mut tab_writer = TabWriter::new(io::stdout());
    writeln!(&mut tab_writer, "NAME\tTYPE\tCLUSTER-IP\tPORT(S)\tAGE")?;

    for svc in services {
        let name = &svc.metadata.name;
        let service_type = &svc.spec.service_type;
        let cluster_ip = svc.spec.cluster_ip.as_deref().unwrap_or("<none>");
        let ports = format_ports(&svc.spec.ports);

        let age = svc
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
            name, service_type, cluster_ip, ports, age
        )?;
    }

    tab_writer.flush()?;
    Ok(())
}

fn format_ports(ports: &[ServicePort]) -> String {
    if ports.is_empty() {
        return "<none>".to_string();
    }

    let mut items = Vec::with_capacity(ports.len());
    for port in ports {
        let protocol = if port.protocol.is_empty() {
            "TCP"
        } else {
            port.protocol.as_str()
        };

        let rendered = match port.node_port {
            Some(node_port) => format!("{}:{}/{}", port.port, node_port, protocol),
            None => format!("{}/{}", port.port, protocol),
        };
        items.push(rendered);
    }

    items.join(",")
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
