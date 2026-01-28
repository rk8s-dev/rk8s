use crate::chuck::BlockStore;
use crate::meta::MetaLayer;
use std::sync::Arc;

pub(crate) struct Backend<B, M> {
    store: Arc<B>,
    meta: Arc<M>,
}

impl<B, M> Backend<B, M>
where
    B: BlockStore,
    M: MetaLayer,
{
    pub(crate) fn new(store: Arc<B>, meta: Arc<M>) -> Self {
        Self { store, meta }
    }

    pub(crate) fn meta(&self) -> &M {
        &self.meta
    }

    pub(crate) fn store(&self) -> &B {
        &self.store
    }
}
