use std::collections::HashMap;

use etcd_client::{Client, GetOptions, KeyValue};

use crate::models::{
    GangSpec, GpuResources, NodeInfo, NodeSpec, PodInfo, PodSpec, QueuedInfo,
    ResourcesRequirements, TopologyConstraint,
};
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

pub fn convert_pod_task_to_pod_info(pod_task: PodTask) -> PodInfo {
    let mut total_cpu = 0;
    let mut total_memory = 0;
    let mut total_gpu: u32 = 0;
    let mut max_gpu_mem: Option<u64> = None;

    for container in &pod_task.spec.containers {
        if let Some(resources) = &container.resources
            && let Some(limits) = &resources.limits
        {
            total_cpu += parse_cpu(&limits.cpu.clone().unwrap_or_default());
            total_memory += parse_memory(&limits.memory.clone().unwrap_or_default());
            total_gpu = total_gpu.saturating_add(limits.gpu.unwrap_or(0));
            if let Some(s) = &limits.gpu_memory {
                let v = parse_memory(s);
                max_gpu_mem = Some(max_gpu_mem.map_or(v, |m| m.max(v)));
            }
        }
    }

    let mut init_cpu = 0;
    let mut init_memory = 0;
    let mut init_gpu: u32 = 0;
    let mut init_gpu_mem: Option<u64> = None;

    for container in &pod_task.spec.init_containers {
        if let Some(resources) = &container.resources
            && let Some(limits) = &resources.limits
        {
            init_cpu = init_cpu.max(parse_cpu(&limits.cpu.clone().unwrap_or_default()));
            init_memory = init_memory.max(parse_memory(&limits.memory.clone().unwrap_or_default()));
            init_gpu = init_gpu.max(limits.gpu.unwrap_or(0));
            if let Some(s) = &limits.gpu_memory {
                let v = parse_memory(s);
                init_gpu_mem = Some(init_gpu_mem.map_or(v, |m| m.max(v)));
            }
        }
    }

    total_cpu = total_cpu.max(init_cpu);
    total_memory = total_memory.max(init_memory);
    total_gpu = total_gpu.max(init_gpu);
    max_gpu_mem = match (max_gpu_mem, init_gpu_mem) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    };

    let gang = pod_task.spec.gang.clone().map(|g| GangSpec {
        id: g.id,
        size: g.size,
    });
    let topology_constraints: Vec<TopologyConstraint> = pod_task
        .spec
        .topology_constraints
        .iter()
        .cloned()
        .map(|c| TopologyConstraint {
            topology_key: c.topology_key,
            same_value: c.same_value,
        })
        .collect();

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
        affinity: pod_task.spec.affinity.map(crate::models::Affinity::from),
        gpu_request: total_gpu,
        gpu_memory_request: max_gpu_mem,
        gang,
        topology_constraints,
    };

    PodInfo {
        name: pod_task.metadata.name,
        labels: pod_task.metadata.labels,
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

    let gpu_resources = labels
        .get("nvidia.com/gpu.count")
        .and_then(|v| v.parse::<u32>().ok())
        .map(|total| GpuResources {
            total,
            requested: 0,
            memory_per_gpu: labels
                .get("nvidia.com/gpu.memory-gib")
                .and_then(|v| v.parse::<u64>().ok())
                .map(|gib| gib * 1024 * 1024 * 1024)
                .unwrap_or(0),
            model: labels
                .get("nvidia.com/gpu.product")
                .cloned()
                .unwrap_or_default(),
        });

    NodeInfo {
        name: k8s_node.metadata.name,
        labels,
        spec,
        requested: ResourcesRequirements::default(),
        allocatable,
        gpu_resources,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pod_with_gpu_and_gang() {
        let yaml = r#"
apiVersion: v1
kind: Pod
metadata:
  name: llm-rank0
  namespace: default
  labels: {}
spec:
  containers:
    - name: trainer
      image: nvidia/cuda
      ports: []
      args: []
      tty: false
      resources:
        limits:
          cpu: "8"
          memory: 16Gi
          gpu: 4
          gpu_memory: 80Gi
  gang:
    id: g1
    size: 4
  topology_constraints:
    - topology_key: topology.rk8s.io/nvlink-domain
      same_value: true
"#;
        let pod_task: PodTask = serde_yaml::from_str(yaml).unwrap();
        let info = convert_pod_task_to_pod_info(pod_task);
        assert_eq!(info.spec.gpu_request, 4);
        assert_eq!(info.spec.gpu_memory_request, Some(80 * 1024 * 1024 * 1024));
        let gang = info.spec.gang.expect("gang missing");
        assert_eq!(gang.id, "g1");
        assert_eq!(gang.size, 4);
        assert_eq!(info.spec.topology_constraints.len(), 1);
        assert_eq!(
            info.spec.topology_constraints[0].topology_key,
            "topology.rk8s.io/nvlink-domain"
        );
        assert!(info.spec.topology_constraints[0].same_value);
    }

    #[test]
    fn parse_node_with_gpu_labels() {
        let yaml = r#"
apiVersion: v1
kind: Node
metadata:
  name: gpu-node
  namespace: default
  labels:
    nvidia.com/gpu.count: "8"
    nvidia.com/gpu.memory-gib: "80"
    nvidia.com/gpu.product: "H100"
spec:
  podCIDR: 10.0.0.0/24
  taints: []
status:
  capacity:
    cpu: "64"
    memory: 256Gi
  allocatable:
    cpu: "64"
    memory: 256Gi
  conditions: []
"#;
        let node: Node = serde_yaml::from_str(yaml).unwrap();
        let info = convert_k8s_node_to_node_info(node);
        let gpu = info.gpu_resources.expect("gpu_resources missing");
        assert_eq!(gpu.total, 8);
        assert_eq!(gpu.memory_per_gpu, 80 * 1024 * 1024 * 1024);
        assert_eq!(gpu.model, "H100");
        assert_eq!(gpu.requested, 0);
    }

    #[test]
    fn parse_node_without_gpu_labels() {
        let yaml = r#"
apiVersion: v1
kind: Node
metadata:
  name: cpu-node
  namespace: default
  labels: {}
spec:
  podCIDR: 10.0.0.0/24
  taints: []
status:
  capacity:
    cpu: "8"
    memory: 16Gi
  allocatable:
    cpu: "8"
    memory: 16Gi
  conditions: []
"#;
        let node: Node = serde_yaml::from_str(yaml).unwrap();
        let info = convert_k8s_node_to_node_info(node);
        assert!(info.gpu_resources.is_none());
    }
}
