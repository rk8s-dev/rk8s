use crate::{
    models::PodInfo,
    plugins::{Code, Plugin, PreEnqueuePlugin, Status},
};

pub struct SchedulingGates;

impl Plugin for SchedulingGates {
    fn name(&self) -> &str {
        "SchedulingGates"
    }
}

impl PreEnqueuePlugin for SchedulingGates {
    fn pre_enqueue(&self, pod: &PodInfo) -> Status {
        if pod.spec.scheduling_gates.is_empty() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{PodSpec, QueuedInfo};

    #[test]
    fn test_scheduling_gates_pre_enqueue_no_gates() {
        let plugin = SchedulingGates;

        let pod = PodInfo {
            labels: std::collections::HashMap::new(),
            name: "test-pod".to_string(),
            spec: PodSpec {
                scheduling_gates: vec![],
                ..Default::default()
            },
            queued_info: QueuedInfo::default(),
            scheduled: None,
        };

        // Should succeed when no scheduling gates
        let result = plugin.pre_enqueue(&pod);
        assert_eq!(result.code, Code::Success);
    }

    #[test]
    fn test_scheduling_gates_pre_enqueue_with_gates() {
        let plugin = SchedulingGates;

        let pod = PodInfo {
            labels: std::collections::HashMap::new(),
            name: "test-pod".to_string(),
            spec: PodSpec {
                scheduling_gates: vec!["gate1".to_string(), "gate2".to_string()],
                ..Default::default()
            },
            queued_info: QueuedInfo::default(),
            scheduled: None,
        };

        // Should fail when scheduling gates are present
        let result = plugin.pre_enqueue(&pod);
        assert_eq!(result.code, Code::UnschedulableAndUnresolvable);
        assert!(result.reasons[0].contains("waiting for scheduling gates"));
        assert!(result.reasons[0].contains("gate1"));
        assert!(result.reasons[0].contains("gate2"));
    }

    #[test]
    fn test_scheduling_gates_plugin_name() {
        let plugin = SchedulingGates;
        assert_eq!(plugin.name(), "SchedulingGates");
    }
}
