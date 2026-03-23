use anyhow::{Error, Result};
use clap::Args;

use crate::commands::container::{list_container, state_container};
use crate::commands::deployment::{deployment_get, deployment_list};
use crate::commands::pod::TLSConnectionArgs;
use crate::commands::pod::{pod_get, pod_list};
use crate::commands::replicaset::{replicaset_get, replicaset_list};
use crate::commands::service::{service_get, service_list};
use tracing::{debug, warn};

#[derive(Args, Debug, Clone)]
pub struct GetCommand {
    /// resources type
    /// pod po pods rs,service pod/[pod-name] [name]
    pub resource: Vec<String>,

    /// subresource type
    #[arg(long)]
    pub subresource: Option<String>,

    /// RKS control-plane address (required for Deployment, ReplicaSet, Service and cluster-mode Pod).
    #[arg(
        long,
        value_name = "RKS_ADDRESS",
        env = "RKS_ADDRESS",
        required = false
    )]
    pub cluster: Option<String>,

    #[clap(flatten)]
    pub tls_cfg: TLSConnectionArgs,
}

pub enum ResourceType {
    Pod,
    Container,
    Deployment,
    ReplicaSet,
    Service,
}

pub struct ResourceArg {
    pub resource_type: ResourceType,
    pub resource_name: Vec<String>,
}

pub fn parse_resource_type(resource_arg: &Vec<String>) -> Vec<ResourceArg> {
    // pattern 1: resource type only, like pod
    // pattern 2: resource type / resource name, like pod/pod-name
    // pattern 3: resource type,resource type, like rs,pod
    // pattern 4: name

    let mut result = Vec::new();
    let mut names = Vec::new();

    for arg in resource_arg {
        debug!("parse_resource_type has arg: {}", arg);
        // pattern 3
        if arg.contains(',') {
            for part in arg.split(',') {
                let part = part.trim();
                if !part.is_empty()
                    && let Some(resource_type) = parse_single_resource_type(part)
                {
                    result.push(ResourceArg {
                        resource_type,
                        resource_name: Vec::new(),
                    });
                }
            }
        }
        // pattern 2
        else if arg.contains('/') {
            let parts: Vec<&str> = arg.splitn(2, '/').collect();
            if parts.len() == 2
                && let Some(resource_type) = parse_single_resource_type(parts[0].trim())
            {
                result.push(ResourceArg {
                    resource_type,
                    resource_name: vec![parts[1].trim().to_string()],
                });
            }
        }
        // pattern 1 & pattern 4
        else if let Some(resource_type) = parse_single_resource_type(arg.trim()) {
            result.push(ResourceArg {
                resource_type,
                resource_name: Vec::new(),
            });
        } else {
            names.push(arg.trim().to_string());
        }
    }

    for res in &mut result {
        res.resource_name.append(&mut names.clone());
    }

    result
}

fn parse_single_resource_type(s: &str) -> Option<ResourceType> {
    match s.to_lowercase().as_str() {
        "pod" | "po" | "pods" => Some(ResourceType::Pod),
        "container" | "c" | "containers" => Some(ResourceType::Container),
        "deployment" | "deploy" | "deployments" => Some(ResourceType::Deployment),
        "replicaset" | "rs" | "replicasets" => Some(ResourceType::ReplicaSet),
        "service" | "svc" | "services" => Some(ResourceType::Service),
        other => {
            warn!("{} is not a known resource type.", other);
            None
        }
    }
}

pub fn get_execute(cmd: GetCommand) -> Result<(), Error> {
    let resource_args = parse_resource_type(&cmd.resource);

    for resource_arg in resource_args {
        if !resource_arg.resource_name.is_empty() {
            // there is specified resource name, do get or state
            for name in resource_arg.resource_name {
                match resource_arg.resource_type {
                    ResourceType::Container => state_container(&name)?,
                    ResourceType::Pod => pod_get(&name, cmd.cluster.clone(), cmd.tls_cfg.clone())?,
                    ResourceType::Deployment => {
                        deployment_get(&name, cmd.cluster.clone(), cmd.tls_cfg.clone())?
                    }
                    ResourceType::ReplicaSet => {
                        replicaset_get(&name, cmd.cluster.clone(), cmd.tls_cfg.clone())?
                    }
                    ResourceType::Service => {
                        service_get(&name, cmd.cluster.clone(), cmd.tls_cfg.clone())?
                    }
                }
            }
        } else {
            // no specified resource name, do list
            match resource_arg.resource_type {
                ResourceType::Container => list_container(None, None)?,
                ResourceType::Pod => pod_list(cmd.cluster.clone(), cmd.tls_cfg.clone())?,
                ResourceType::Deployment => {
                    deployment_list(cmd.cluster.clone(), cmd.tls_cfg.clone())?
                }
                ResourceType::ReplicaSet => {
                    replicaset_list(cmd.cluster.clone(), cmd.tls_cfg.clone())?
                }
                ResourceType::Service => service_list(cmd.cluster.clone(), cmd.tls_cfg.clone())?,
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // fn resource_names(args: &[ResourceArg]) -> Vec<Vec<String>> {
    //     args.iter().map(|a| a.resource_name.clone()).collect()
    // }

    fn resource_types(args: &[ResourceArg]) -> Vec<&str> {
        args.iter()
            .map(|a| match a.resource_type {
                ResourceType::Pod => "pod",
                ResourceType::Container => "container",
                ResourceType::Deployment => "deployment",
                ResourceType::ReplicaSet => "replicaset",
                ResourceType::Service => "service",
            })
            .collect()
    }

    // pattern 1: resource type only
    #[test]
    fn test_resource_type_only() {
        let result = parse_resource_type(&vec!["pod".to_string()]);
        assert_eq!(result.len(), 1);
        assert_eq!(resource_types(&result), vec!["pod"]);
        assert!(result[0].resource_name.is_empty());
    }

    // pattern 1: all aliases
    #[test]
    fn test_resource_type_aliases() {
        let cases = vec![
            (vec!["po"], "pod"),
            (vec!["pods"], "pod"),
            (vec!["container"], "container"),
            (vec!["c"], "container"),
            (vec!["containers"], "container"),
            (vec!["deploy"], "deployment"),
            (vec!["deployments"], "deployment"),
            (vec!["rs"], "replicaset"),
            (vec!["replicasets"], "replicaset"),
            (vec!["svc"], "service"),
            (vec!["services"], "service"),
        ];
        for (input, expected_type) in cases {
            let args: Vec<String> = input.iter().map(|s| s.to_string()).collect();
            let result = parse_resource_type(&args);
            assert_eq!(result.len(), 1, "input: {:?}", input);
            assert_eq!(
                resource_types(&result)[0],
                expected_type,
                "input: {:?}",
                input
            );
        }
    }

    // pattern 2: resource type / resource name
    #[test]
    fn test_resource_type_slash_name() {
        let result = parse_resource_type(&vec!["pod/my-pod".to_string()]);
        assert_eq!(result.len(), 1);
        assert_eq!(resource_types(&result), vec!["pod"]);
        assert_eq!(result[0].resource_name, vec!["my-pod"]);
    }

    // pattern 2: name with slashes should only split on first slash
    #[test]
    fn test_resource_type_slash_name_with_extra_slash() {
        let result = parse_resource_type(&vec!["deployment/my-app/extra".to_string()]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].resource_name, vec!["my-app/extra"]);
    }

    // pattern 3: comma-separated types
    #[test]
    fn test_comma_separated_types() {
        let result = parse_resource_type(&vec!["pod,rs".to_string()]);
        assert_eq!(result.len(), 2);
        let types = resource_types(&result);
        assert!(types.contains(&"pod"));
        assert!(types.contains(&"replicaset"));
    }

    // pattern 4: bare name (unknown type) is attached to preceding resource types
    #[test]
    fn test_bare_name_appended_to_types() {
        let result = parse_resource_type(&vec!["pod".to_string(), "my-pod".to_string()]);
        assert_eq!(result.len(), 1);
        assert_eq!(resource_types(&result), vec!["pod"]);
        assert_eq!(result[0].resource_name, vec!["my-pod"]);
    }

    // multiple bare names appended to multiple types
    #[test]
    fn test_multiple_names_appended_to_multiple_types() {
        let result = parse_resource_type(&vec![
            "pod,rs".to_string(),
            "foo".to_string(),
            "bar".to_string(),
        ]);
        assert_eq!(result.len(), 2);
        for arg in &result {
            assert_eq!(arg.resource_name, vec!["foo", "bar"]);
        }
    }

    // unknown-only input produces no results
    #[test]
    fn test_unknown_type_only() {
        let result = parse_resource_type(&vec!["unknownresource".to_string()]);
        // unknown types become bare names but with no resource type to attach to
        assert!(result.is_empty());
    }

    // empty input
    #[test]
    fn test_empty_input() {
        let result = parse_resource_type(&vec![]);
        assert!(result.is_empty());
    }

    // case-insensitive matching
    #[test]
    fn test_case_insensitive() {
        let result = parse_resource_type(&vec!["POD".to_string()]);
        assert_eq!(result.len(), 1);
        assert_eq!(resource_types(&result), vec!["pod"]);
    }
}
