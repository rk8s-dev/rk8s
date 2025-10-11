use super::{Backend, Field, Operation, field::IntoFieldArc, request::Request, response::Response};
use crate::{context::Context, errors::RvError};
use std::{collections::HashMap, fmt, future::Future, pin::Pin, sync::Arc};

pub type PathOperationFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Option<Response>, RvError>> + Send + 'a>>;
type PathOperationHandler =
    dyn for<'a> Fn(&'a dyn Backend, &'a mut Request) -> PathOperationFuture<'a> + Send + Sync;

#[derive(Debug, Clone)]
pub struct Path {
    pub ctx: Arc<Context>,
    pub pattern: String,
    pub fields: HashMap<String, Arc<Field>>,
    pub operations: Vec<PathOperation>,
    pub help: String,
}

impl Default for Path {
    fn default() -> Self {
        Self {
            ctx: Arc::new(Context::new()),
            pattern: String::new(),
            fields: HashMap::new(),
            operations: Vec::new(),
            help: String::new(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PathBuilder {
    path: Path,
}

impl PathBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn context(mut self, ctx: Arc<Context>) -> Self {
        self.path.ctx = ctx;
        self
    }

    pub fn pattern(mut self, pattern: impl Into<String>) -> Self {
        self.path.pattern = pattern.into();
        self
    }

    pub fn help(mut self, help: impl Into<String>) -> Self {
        self.path.help = help.into();
        self
    }

    pub fn fields(mut self, fields: HashMap<String, Arc<Field>>) -> Self {
        self.path.fields = fields;
        self
    }

    pub fn field<F>(mut self, name: impl Into<String>, field: F) -> Self
    where
        F: IntoFieldArc,
    {
        self.path.fields.insert(name.into(), field.into_field_arc());
        self
    }

    pub fn operations(mut self, operations: Vec<PathOperation>) -> Self {
        self.path.operations = operations;
        self
    }

    pub fn operation<H>(mut self, op: Operation, handler: H) -> Self
    where
        H: for<'a> Fn(&'a dyn Backend, &'a mut Request) -> PathOperationFuture<'a>
            + Send
            + Sync
            + 'static,
    {
        self.path
            .operations
            .push(PathOperation::with_handler(op, handler));
        self
    }

    pub fn operation_entry(mut self, operation: PathOperation) -> Self {
        self.path.operations.push(operation);
        self
    }

    pub fn build(self) -> Path {
        self.path
    }
}

#[derive(Clone)]
pub struct PathOperation {
    pub op: Operation,
    pub handler: Arc<PathOperationHandler>,
}

impl fmt::Debug for PathOperation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PathOperation")
            .field("op", &self.op)
            .finish()
    }
}

impl PathOperation {
    pub fn with_handler<H>(op: Operation, handler: H) -> Self
    where
        H: for<'a> Fn(&'a dyn Backend, &'a mut Request) -> PathOperationFuture<'a>
            + Send
            + Sync
            + 'static,
    {
        Self {
            op,
            handler: Arc::new(handler),
        }
    }
}

impl Path {
    pub fn new(pattern: &str) -> Self {
        Self {
            pattern: pattern.to_string(),
            ..Self::default()
        }
    }

    pub fn builder() -> PathBuilder {
        PathBuilder::new()
    }

    pub fn get_field(&self, key: &str) -> Option<Arc<Field>> {
        self.fields.get(key).cloned()
    }
}

impl PathOperation {
    pub fn new() -> Self {
        Self {
            op: Operation::Read,
            handler: Arc::new(|_backend, _req| Box::pin(async move { Ok(None) })),
        }
    }

    pub async fn handle_request(
        &self,
        backend: &dyn Backend,
        req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        (self.handler)(backend, req).await
    }
}
