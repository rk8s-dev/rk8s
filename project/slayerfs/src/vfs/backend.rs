use crate::chuck::BlockStore;
use crate::meta::MetaStore;
use std::sync::Arc;

pub struct Backend<B, M> {
    store: Arc<B>,
    meta: Arc<M>,
}

impl<B, M> Backend<B, M>
where
    B: BlockStore,
    M: MetaStore,
{
    pub fn new(store: Arc<B>, meta: Arc<M>) -> Self {
        Self { store, meta }
    }

    pub fn meta(&self) -> &M {
        &self.meta
    }

    pub fn store(&self) -> &B {
        &self.store
    }
}
