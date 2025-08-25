use crate::{
    cycle_state::CycleState,
    models::{NodeInfo, PodInfo, Taint, TaintEffect, TaintKey},
    plugins::{
        FilterPlugin, Plugin,
        Code, Status,
    },
};

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
        return Status::new(
            Code::UnschedulableAndUnresolvable,
            vec!["node(s) were unschedulable".to_string()],
        );
    }
}
