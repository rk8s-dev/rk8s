use std::collections::HashMap;

use etcd_client::{Client, GetOptions, KeyValue};

use crate::models::{NodeInfo, NodeSpec, PodInfo, PodSpec, QueuedInfo, ResourcesRequirements};
use common::{Node, PodTask};

pub async fn get_pod(
    client: &mut Client,
    pod_name: &str,
) -> Result<Option<PodTask>, anyhow::Error> {
    let key = format!("/registry/pods/{pod_name}");
    let resp = client.get(key, None).await?;
    let resp = resp.kvs().first().map(|kv| kv.value());
    if let Some(pod_yaml) = resp {
        let pod: PodTask = serde_yaml::from_slice(pod_yaml)?;
        Ok(Some(pod))
    } else {
        Ok(None)
    }
}

pub async fn list_pods(client: &mut Client) -> Result<Vec<PodInfo>, anyhow::Error> {
    let resp = client
        .get("/registry/pods/", Some(GetOptions::new().with_prefix()))
        .await?;
    let list: Vec<_> = resp
        .kvs()
        .iter()
        .map(|kv| String::from_utf8_lossy(kv.key()).replace("/registry/pods/", ""))
        .collect();
    let mut res = Vec::new();
    for name in list {
        let pod = get_pod(client, &name).await?;
        if let Some(p) = pod {
            res.push(convert_pod_task_to_pod_info(p))
        }
    }
    Ok(res)
}

pub async fn list_nodes(client: &mut Client) -> Result<Vec<NodeInfo>, anyhow::Error> {
    let resp = client
        .get("/registry/nodes/", Some(GetOptions::new().with_prefix()))
        .await?;
    let mut result = Vec::new();
    for kv in resp.kvs() {
        let node: Node = serde_yaml::from_slice(kv.value())?;
        result.push(convert_k8s_node_to_node_info(node));
    }
    Ok(result)
}

pub fn get_pod_from_kv(kv: &KeyValue) -> Result<PodInfo, anyhow::Error> {
    let value = kv.value();
    let pod_task: PodTask = serde_yaml::from_slice(value)?;
    Ok(convert_pod_task_to_pod_info(pod_task))
}

pub fn get_node_from_kv(kv: &KeyValue) -> Result<NodeInfo, anyhow::Error> {
    let value = kv.value();
    let pod_task: Node = serde_yaml::from_slice(value)?;
    Ok(convert_k8s_node_to_node_info(pod_task))
}

fn convert_pod_task_to_pod_info(pod_task: PodTask) -> PodInfo {
    let mut total_cpu = 0;
    let mut total_memory = 0;

    for container in &pod_task.spec.containers {
        if let Some(resources) = &container.resources
            && let Some(limits) = &resources.limits
        {
            total_cpu += parse_cpu(&limits.cpu.clone().unwrap_or_default());
            total_memory += parse_memory(&limits.memory.clone().unwrap_or_default());
        }
    }

    let mut init_cpu = 0;
    let mut init_memory = 0;

    for container in &pod_task.spec.init_containers {
        if let Some(resources) = &container.resources
            && let Some(limits) = &resources.limits
        {
            init_cpu = init_cpu.max(parse_cpu(&limits.cpu.clone().unwrap_or_default()));
            init_memory = init_memory.max(parse_memory(&limits.memory.clone().unwrap_or_default()));
        }
    }

    total_cpu = total_cpu.max(init_cpu);
    total_memory = total_memory.max(init_memory);

    let spec = PodSpec {
        resources: ResourcesRequirements {
            cpu: total_cpu,
            memory: total_memory,
        },
        priority: 0,
        scheduling_gates: Vec::new(),
        tolerations: pod_task.spec.tolerations,
        node_name: pod_task.spec.node_name.clone(),
        node_selector: HashMap::new(),
        affinity: None,
    };

    PodInfo {
        name: pod_task.metadata.name,
        spec,
        queued_info: QueuedInfo::default(),
        scheduled: pod_task.spec.node_name.clone(),
    }
}

fn convert_k8s_node_to_node_info(k8s_node: Node) -> NodeInfo {
    let labels = k8s_node.metadata.labels;

    let spec = NodeSpec {
        unschedulable: false,
        taints: k8s_node.spec.taints,
    };

    let allocatable = ResourcesRequirements {
        cpu: parse_cpu(
            k8s_node
                .status
                .allocatable
                .get("cpu")
                .unwrap_or(&"0".to_string()),
        ),
        memory: parse_memory(
            k8s_node
                .status
                .allocatable
                .get("memory")
                .unwrap_or(&"0".to_string()),
        ),
    };

    NodeInfo {
        name: k8s_node.metadata.name,
        labels,
        spec,
        requested: ResourcesRequirements::default(),
        allocatable,
    }
}

fn parse_cpu(cpu_str: &str) -> u64 {
    if cpu_str.ends_with('m') {
        cpu_str.trim_end_matches('m').parse::<u64>().unwrap_or(0)
    } else {
        (cpu_str.parse::<f64>().unwrap_or(0.0) * 1000.0) as u64
    }
}

fn parse_memory(memory_str: &str) -> u64 {
    let memory_str = memory_str.to_lowercase();
    if memory_str.ends_with("ki") {
        memory_str
            .trim_end_matches("ki")
            .parse::<u64>()
            .unwrap_or(0)
            * 1024
    } else if memory_str.ends_with("mi") {
        memory_str
            .trim_end_matches("mi")
            .parse::<u64>()
            .unwrap_or(0)
            * 1024
            * 1024
    } else if memory_str.ends_with("gi") {
        memory_str
            .trim_end_matches("gi")
            .parse::<u64>()
            .unwrap_or(0)
            * 1024
            * 1024
            * 1024
    } else if memory_str.ends_with('k') {
        memory_str.trim_end_matches('k').parse::<u64>().unwrap_or(0) * 1000
    } else if memory_str.ends_with('m') {
        memory_str.trim_end_matches('m').parse::<u64>().unwrap_or(0) * 1000 * 1000
    } else if memory_str.ends_with('g') {
        memory_str.trim_end_matches('g').parse::<u64>().unwrap_or(0) * 1000 * 1000 * 1000
    } else {
        memory_str.parse::<u64>().unwrap_or(0)
    }
}
