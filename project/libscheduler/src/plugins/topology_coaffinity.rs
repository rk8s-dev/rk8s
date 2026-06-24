use std::collections::HashMap;
use std::sync::Arc;

use crate::{
    cycle_state::CycleState,
    gang_state::GangStateStore,
    models::{NodeInfo, PodInfo, TopologyConstraint},
    plugins::{Code, FilterPlugin, Plugin, PreFilterPlugin, PreFilterResult, Status},
};

pub struct TopologyCoAffinityFilter {
    pub gang_state: Arc<GangStateStore>,
}

impl Plugin for TopologyCoAffinityFilter {
    fn name(&self) -> &str {
        "TopologyCoAffinityFilter"
    }
}

const STATE_KEY: &str = "PreFilterTopologyCoAffinity";

struct PreFilterCtx {
    required_values: HashMap<String, String>,
}

fn pick_label_value(
    assumed_node_names: &[String],
    key: &str,
    nodes: &[NodeInfo],
) -> Option<String> {
    for name in assumed_node_names {
        if let Some(node) = nodes.iter().find(|n| &n.name == name)
            && let Some(v) = node.labels.get(key)
        {
            return Some(v.clone());
        }
    }
    None
}

impl PreFilterPlugin for TopologyCoAffinityFilter {
    fn pre_filter(
        &self,
        state: &mut CycleState,
        pod: &PodInfo,
        nodes: Vec<NodeInfo>,
    ) -> (PreFilterResult, Status) {
        let Some(gang) = pod.spec.gang.as_ref() else {
            return (
                PreFilterResult { node_names: vec![] },
                Status::new(Code::Skip, vec![]),
            );
        };
        if pod.spec.topology_constraints.is_empty() {
            return (
                PreFilterResult { node_names: vec![] },
                Status::new(Code::Skip, vec![]),
            );
        }

        let assumed = self.gang_state.assumed_nodes(&gang.id);
        let mut required = HashMap::new();
        if !assumed.is_empty() {
            for c in pod.spec.topology_constraints.iter() {
                let TopologyConstraint {
                    topology_key,
                    same_value,
                } = c;
                if !*same_value {
                    continue;
                }
                if let Some(v) = pick_label_value(&assumed, topology_key, &nodes) {
                    required.insert(topology_key.clone(), v);
                } else {
                    return (
                        PreFilterResult { node_names: vec![] },
                        Status::new(
                            Code::Unschedulable,
                            vec!["assumed gang member missing topology label".into()],
                        ),
                    );
                }
            }
        }

        state.write(
            STATE_KEY,
            Box::new(PreFilterCtx {
                required_values: required,
            }),
        );
        (PreFilterResult { node_names: vec![] }, Status::default())
    }
}

impl FilterPlugin for TopologyCoAffinityFilter {
    fn filter(&self, state: &mut CycleState, _pod: &PodInfo, node: NodeInfo) -> Status {
        let Some(ctx) = state.read::<PreFilterCtx>(STATE_KEY) else {
            return Status::default();
        };
        if ctx.required_values.is_empty() {
            return Status::default();
        }
        for (k, v) in ctx.required_values.iter() {
            match node.labels.get(k) {
                Some(nv) if nv == v => {}
                _ => {
                    return Status::new(
                        Code::Unschedulable,
                        vec![format!("node label {k} mismatch for gang topology")],
                    );
                }
            }
        }
        Status::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cycle_state::CycleState;
    use crate::gang_state::GangStateStore;
    use crate::models::{GangSpec, PodSpec, QueuedInfo, TopologyConstraint};
    use std::collections::HashMap;

    fn pod_with_gang(gang_id: &str, key: &str) -> PodInfo {
        PodInfo {
            name: "p".into(),
            labels: HashMap::new(),
            spec: PodSpec {
                gang: Some(GangSpec {
                    id: gang_id.into(),
                    size: 4,
                }),
                topology_constraints: vec![TopologyConstraint {
                    topology_key: key.into(),
                    same_value: true,
                }],
                ..Default::default()
            },
            queued_info: QueuedInfo::default(),
            scheduled: None,
        }
    }

    fn node(name: &str, key: &str, value: &str) -> NodeInfo {
        let mut labels = HashMap::new();
        labels.insert(key.into(), value.into());
        NodeInfo {
            name: name.into(),
            labels,
            ..Default::default()
        }
    }

    #[test]
    fn pod_without_gang_short_circuits_skip() {
        let plugin = TopologyCoAffinityFilter {
            gang_state: Arc::new(GangStateStore::default()),
        };
        let mut state = CycleState::default();
        let pod = PodInfo::default();
        let (_, sta) = plugin.pre_filter(&mut state, &pod, vec![]);
        assert_eq!(sta.code, Code::Skip);
    }

    #[test]
    fn first_pod_no_constraint() {
        let plugin = TopologyCoAffinityFilter {
            gang_state: Arc::new(GangStateStore::default()),
        };
        let mut state = CycleState::default();
        let pod = pod_with_gang("g1", "topology.rk8s.io/nvlink-domain");
        let n_a = node("a", "topology.rk8s.io/nvlink-domain", "domain0");
        let n_b = node("b", "topology.rk8s.io/nvlink-domain", "domain1");

        let (_, sta) = plugin.pre_filter(&mut state, &pod, vec![n_a.clone(), n_b.clone()]);
        assert_eq!(sta.code, Code::Success);
        assert_eq!(plugin.filter(&mut state, &pod, n_a).code, Code::Success);
        assert_eq!(plugin.filter(&mut state, &pod, n_b).code, Code::Success);
    }

    #[test]
    fn second_pod_constrained_to_first_member_topology() {
        let store = Arc::new(GangStateStore::default());
        store.add_member("g1", 4, "p1", "node-a");
        let plugin = TopologyCoAffinityFilter { gang_state: store };
        let mut state = CycleState::default();
        let pod = pod_with_gang("g1", "topology.rk8s.io/nvlink-domain");
        let n_a = node("node-a", "topology.rk8s.io/nvlink-domain", "domain0");
        let n_b = node("node-b", "topology.rk8s.io/nvlink-domain", "domain1");

        let (_, sta) = plugin.pre_filter(&mut state, &pod, vec![n_a.clone(), n_b.clone()]);
        assert_eq!(sta.code, Code::Success);
        assert_eq!(plugin.filter(&mut state, &pod, n_a).code, Code::Success);
        assert_eq!(
            plugin.filter(&mut state, &pod, n_b).code,
            Code::Unschedulable
        );
    }

    #[test]
    fn missing_topology_label_on_assumed_node_treats_as_unschedulable() {
        let store = Arc::new(GangStateStore::default());
        store.add_member("g1", 4, "p1", "node-a");
        let plugin = TopologyCoAffinityFilter { gang_state: store };
        let mut state = CycleState::default();
        let pod = pod_with_gang("g1", "topology.rk8s.io/nvlink-domain");
        // node-a is provided in the snapshot but lacks the label.
        let n_a_no_label = NodeInfo {
            name: "node-a".into(),
            ..Default::default()
        };

        let (_, sta) = plugin.pre_filter(&mut state, &pod, vec![n_a_no_label]);
        assert_eq!(sta.code, Code::Unschedulable);
    }
}
