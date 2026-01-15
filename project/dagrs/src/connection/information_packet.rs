use std::{any::Any, sync::Arc};

/// Container type to store task output.
#[derive(Debug, Clone)]
pub struct Content {
    pub inner: Arc<dyn Any + Send + Sync>,
}

impl Content {
    /// Construct a new [`Content`].
    pub fn new<H: Send + Sync + 'static>(val: H) -> Self {
        Self {
            inner: Arc::new(val),
        }
    }

    pub fn from_arc<H: Send + Sync + 'static>(val: Arc<H>) -> Self {
        Self { inner: val }
    }

    pub fn get<H: 'static>(&self) -> Option<&H> {
        self.inner.downcast_ref::<H>()
    }

    pub fn into_inner<H: Send + Sync + 'static>(self) -> Option<Arc<H>> {
        self.inner.downcast::<H>().ok()
    }
}
