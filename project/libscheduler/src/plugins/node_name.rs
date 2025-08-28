use crate::{
    cycle_state::CycleState,
    plugins::{
        ActionType, ClusterEvent, ClusterEventWithHint, Code, EnqueueExtension, EventResource, FilterPlugin, Plugin, Status
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
