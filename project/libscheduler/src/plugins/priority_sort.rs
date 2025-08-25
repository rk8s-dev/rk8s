use std::cmp::Ordering;

use crate::{
    models::PodInfo,
    plugins::{Plugin, QueueSortPlugin},
};

pub struct PrioritySort;

impl Plugin for PrioritySort {
    fn name(&self) -> &str {
        "PrioritySort"
    }
}

impl QueueSortPlugin for PrioritySort {
    fn less(&self, a: PodInfo, b: PodInfo) -> Ordering {
        match a.spec.priority.cmp(&b.spec.priority) {
            Ordering::Less => Ordering::Less,
            Ordering::Greater => Ordering::Greater,
            Ordering::Equal => a.queued_info.timestamp.cmp(&b.queued_info.timestamp),
        }
    }
}
