use crate::{
    cycle_state::CycleState,
    models::{NodeInfo, PodInfo},
    plugins::{Code, FilterPlugin, Plugin, Status},
};
use common::{Taint, TaintEffect, TaintKey};
pub struct NodeUnschedulable;

impl Plugin for NodeUnschedulable {
    fn name(&self) -> &str {
        "NodeUnschedulable"
    }
}

impl FilterPlugin for NodeUnschedulable {
    fn filter(&self, _: &mut CycleState, pod: &PodInfo, node_info: NodeInfo) -> Status {
        if !node_info.spec.unschedulable {
            return Status::default();
        }
        for toleration in &pod.spec.tolerations {
            if toleration.tolerate(&Taint::new(
                TaintKey::NodeUnschedulable,
                TaintEffect::NoSchedule,
            )) {
                return Status::default();
            }
        }
        Status::new(
            Code::UnschedulableAndUnresolvable,
            vec!["node(s) were unschedulable".to_string()],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cycle_state::CycleState;
    use crate::models::{NodeSpec, PodSpec, QueuedInfo};
    use common::{TaintEffect, Toleration, TolerationOperator};
    #[test]
    fn test_node_unschedulable_filter_schedulable_node() {
        let plugin = NodeUnschedulable;
        let mut state = CycleState::default();

        let pod = PodInfo {
            labels: std::collections::HashMap::new(),
            name: "test-pod".to_string(),
            spec: PodSpec::default(),
            queued_info: QueuedInfo::default(),
            scheduled: None,
        };

        let node = NodeInfo {
            name: "test-node".to_string(),
            spec: NodeSpec {
                unschedulable: false,
                taints: vec![],
            },
            ..Default::default()
        };

        // Should succeed when node is schedulable
        let result = plugin.filter(&mut state, &pod, node);
        assert_eq!(result.code, Code::Success);
    }

    #[test]
    fn test_node_unschedulable_filter_unschedulable_node_no_toleration() {
        let plugin = NodeUnschedulable;
        let mut state = CycleState::default();

        let pod = PodInfo {
            labels: std::collections::HashMap::new(),
            name: "test-pod".to_string(),
            spec: PodSpec::default(),
            queued_info: QueuedInfo::default(),
            scheduled: None,
        };

        let node = NodeInfo {
            name: "test-node".to_string(),
            spec: NodeSpec {
                unschedulable: true,
                taints: vec![],
            },
            ..Default::default()
        };

        // Should fail when node is unschedulable and pod has no toleration
        let result = plugin.filter(&mut state, &pod, node);
        assert_eq!(result.code, Code::UnschedulableAndUnresolvable);
        assert!(
            result
                .reasons
                .contains(&"node(s) were unschedulable".to_string())
        );
    }

    #[test]
    fn test_node_unschedulable_filter_unschedulable_node_with_toleration() {
        let plugin = NodeUnschedulable;
        let mut state = CycleState::default();

        let pod = PodInfo {
            labels: std::collections::HashMap::new(),
            name: "test-pod".to_string(),
            spec: PodSpec {
                tolerations: vec![Toleration {
                    key: Some(TaintKey::NodeUnschedulable),
                    operator: TolerationOperator::Exists,
                    value: "".to_string(),
                    effect: Some(TaintEffect::NoSchedule),
                }],
                ..Default::default()
            },
            queued_info: QueuedInfo::default(),
            scheduled: None,
        };

        let node = NodeInfo {
            name: "test-node".to_string(),
            spec: NodeSpec {
                unschedulable: true,
                taints: vec![],
            },
            ..Default::default()
        };

        // Should succeed when node is unschedulable but pod has toleration
        let result = plugin.filter(&mut state, &pod, node);
        assert_eq!(result.code, Code::Success);
    }

    #[test]
    fn test_node_unschedulable_filter_unschedulable_node_wrong_toleration() {
        let plugin = NodeUnschedulable;
        let mut state = CycleState::default();

        let pod = PodInfo {
            labels: std::collections::HashMap::new(),
            name: "test-pod".to_string(),
            spec: PodSpec {
                tolerations: vec![Toleration {
                    key: Some(TaintKey::NodeNotReady), // Wrong taint key
                    operator: TolerationOperator::Exists,
                    value: "".to_string(),
                    effect: Some(TaintEffect::NoSchedule),
                }],
                ..Default::default()
            },
            queued_info: QueuedInfo::default(),
            scheduled: None,
        };

        let node = NodeInfo {
            name: "test-node".to_string(),
            spec: NodeSpec {
                unschedulable: true,
                taints: vec![],
            },
            ..Default::default()
        };

        // Should fail when node is unschedulable and pod has wrong toleration
        let result = plugin.filter(&mut state, &pod, node);
        assert_eq!(result.code, Code::UnschedulableAndUnresolvable);
        assert!(
            result
                .reasons
                .contains(&"node(s) were unschedulable".to_string())
        );
    }

    #[test]
    fn test_node_unschedulable_plugin_name() {
        let plugin = NodeUnschedulable;
        assert_eq!(plugin.name(), "NodeUnschedulable");
    }
}
