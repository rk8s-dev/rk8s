use crate::{
    cycle_state::CycleState,
    plugins::{
        ActionType, ClusterEvent, ClusterEventWithHint, Code, EnqueueExtension, EventResource,
        FilterPlugin, Plugin, Status,
    },
};

pub struct NodeName;

impl Plugin for NodeName {
    fn name(&self) -> &str {
        "NodeName"
    }
}

impl FilterPlugin for NodeName {
    fn filter(
        &self,
        _: &mut CycleState,
        pod: &crate::models::PodInfo,
        node_info: crate::models::NodeInfo,
    ) -> Status {
        if pod.spec.node_name.is_none() || pod.spec.node_name.as_ref().unwrap() == &node_info.name {
            Status::default()
        } else {
            Status::new(
                Code::UnschedulableAndUnresolvable,
                vec!["node(s) didn't match the requested node name".to_string()],
            )
        }
    }
}

impl EnqueueExtension for NodeName {
    fn events_to_register(&self) -> Vec<ClusterEventWithHint> {
        // Differ to Kubernetes, we don't have preCheck mechanism now.
        // So directly return event with action type Add.
        vec![ClusterEventWithHint {
            event: ClusterEvent {
                resource: EventResource::Node,
                action_type: ActionType::Add,
            },
            queueing_hint_fn: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cycle_state::CycleState;
    use crate::models::{NodeInfo, PodInfo, PodSpec, QueuedInfo};

    #[test]
    fn test_node_name_filter_no_node_name_specified() {
        let plugin = NodeName;
        let mut state = CycleState::default();

        let pod = PodInfo {
            labels: std::collections::HashMap::new(),
            name: "test-pod".to_string(),
            spec: PodSpec {
                node_name: None,
                ..Default::default()
            },
            queued_info: QueuedInfo::default(),
            scheduled: None,
        };

        let node = NodeInfo {
            name: "any-node".to_string(),
            ..Default::default()
        };

        // Should succeed when no node name is specified
        let result = plugin.filter(&mut state, &pod, node);
        assert_eq!(result.code, Code::Success);
    }

    #[test]
    fn test_node_name_filter_matching_node_name() {
        let plugin = NodeName;
        let mut state = CycleState::default();

        let pod = PodInfo {
            labels: std::collections::HashMap::new(),
            name: "test-pod".to_string(),
            spec: PodSpec {
                node_name: Some("specific-node".to_string()),
                ..Default::default()
            },
            queued_info: QueuedInfo::default(),
            scheduled: None,
        };

        let matching_node = NodeInfo {
            name: "specific-node".to_string(),
            ..Default::default()
        };

        // Should succeed when node name matches
        let result = plugin.filter(&mut state, &pod, matching_node);
        assert_eq!(result.code, Code::Success);
    }

    #[test]
    fn test_node_name_filter_non_matching_node_name() {
        let plugin = NodeName;
        let mut state = CycleState::default();

        let pod = PodInfo {
            labels: std::collections::HashMap::new(),
            name: "test-pod".to_string(),
            spec: PodSpec {
                node_name: Some("specific-node".to_string()),
                ..Default::default()
            },
            queued_info: QueuedInfo::default(),
            scheduled: None,
        };

        let non_matching_node = NodeInfo {
            name: "different-node".to_string(),
            ..Default::default()
        };

        // Should fail when node name doesn't match
        let result = plugin.filter(&mut state, &pod, non_matching_node);
        assert_eq!(result.code, Code::UnschedulableAndUnresolvable);
        assert!(
            result
                .reasons
                .contains(&"node(s) didn't match the requested node name".to_string())
        );
    }

    #[test]
    fn test_node_name_events_to_register() {
        let plugin = NodeName;
        let events = plugin.events_to_register();

        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert!(matches!(event.event.resource, EventResource::Node));
        assert!(event.queueing_hint_fn.is_none());
        assert!(event.event.action_type.contains(ActionType::Add));
    }

    #[test]
    fn test_node_name_plugin_name() {
        let plugin = NodeName;
        assert_eq!(plugin.name(), "NodeName");
    }
}
