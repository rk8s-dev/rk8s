use std::sync::atomic::AtomicUsize;

use super::node::NodeId;

/// IDAllocator for Node.
struct IDAllocator {
    id: AtomicUsize,
}

impl IDAllocator {
    fn alloc(&self) -> NodeId {
        let origin = self.id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if origin > self.id.load(std::sync::atomic::Ordering::Relaxed) {
            panic!("Too many tasks.")
        } else {
            NodeId(origin)
        }
    }
}

/// The global task uniquely identifies an instance of the allocator.
static ID_ALLOCATOR: IDAllocator = IDAllocator {
    id: AtomicUsize::new(1),
};

/// Assign node's id.
pub(crate) fn alloc_id() -> NodeId {
    ID_ALLOCATOR.alloc()
}
