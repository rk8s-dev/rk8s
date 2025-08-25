use crate::{
    models::PodInfo,
    plugins::{
        Code, Status, Plugin, PreEnqueuePlugin
    },
};

pub struct SchedulingGates;

impl Plugin for SchedulingGates {
    fn name(&self) -> &str {
        "SchedulingGates"
    }
}

impl PreEnqueuePlugin for SchedulingGates {
    fn pre_enqueue(&self, pod: &PodInfo) -> Status {
        if pod.spec.scheduling_gates.len() == 0 {
            Status::default()
        } else {
            Status::new(
                Code::UnschedulableAndUnresolvable,
                vec![format!(
                    "waiting for scheduling gates: {:?}",
                    pod.spec.scheduling_gates
                )],
            )
        }
    }
}
