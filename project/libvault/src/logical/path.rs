use super::{Backend, Field, Operation, field::IntoFieldArc, request::Request, response::Response};
use crate::{context::Context, errors::RvError};
#[cfg(not(feature = "sync_handler"))]
use std::future::Future;
#[cfg(not(feature = "sync_handler"))]
use std::pin::Pin;
use std::{collections::HashMap, fmt, sync::Arc};

#[cfg(not(feature = "sync_handler"))]
pub type PathOperationFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Option<Response>, RvError>> + Send + 'a>>;
#[cfg(not(feature = "sync_handler"))]
type PathOperationHandler =
    dyn for<'a> Fn(&'a dyn Backend, &'a mut Request) -> PathOperationFuture<'a> + Send + Sync;
#[cfg(feature = "sync_handler")]
type PathOperationHandler =
    dyn Fn(&dyn Backend, &mut Request) -> Result<Option<Response>, RvError> + Send + Sync;

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

#[derive(Debug, Clone)]
pub struct PathBuilder {
    path: Path,
}

impl Default for PathBuilder {
    fn default() -> Self {
        Self {
            path: Path::default(),
        }
    }
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

    #[cfg(not(feature = "sync_handler"))]
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

    #[cfg(feature = "sync_handler")]
    pub fn operation<H>(mut self, op: Operation, handler: H) -> Self
    where
        H: Fn(&dyn Backend, &mut Request) -> Result<Option<Response>, RvError>
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
    #[cfg(not(feature = "sync_handler"))]
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

    #[cfg(feature = "sync_handler")]
    pub fn with_handler<H>(op: Operation, handler: H) -> Self
    where
        H: Fn(&dyn Backend, &mut Request) -> Result<Option<Response>, RvError>
            + Send
            + Sync
            + 'static,
    {
        let handler = Arc::new(move |backend, req| handler(backend, req));

        Self { op, handler }
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

#[maybe_async::maybe_async]
impl PathOperation {
    #[cfg(not(feature = "sync_handler"))]
    pub fn new() -> Self {
        Self {
            op: Operation::Read,
            handler: Arc::new(|_backend, _req| Box::pin(async move { Ok(None) })),
        }
    }
    #[cfg(feature = "sync_handler")]
    pub fn new() -> Self {
        Self {
            op: Operation::Read,
            handler: Arc::new(|_backend, _req| Ok(None)),
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

#[cfg(test)]
mod test {
    use super::{super::FieldType, *};

    #[maybe_async::maybe_async]
    pub async fn my_test_read_handler(
        _backend: &dyn Backend,
        _req: &mut Request,
    ) -> Result<Option<Response>, RvError> {
        Ok(None)
    }

    #[test]
    #[cfg(not(feature = "sync_handler"))]
    fn test_logical_path() {
        let path = Path::builder()
            .pattern("/aa")
            .field(
                "mytype",
                Field::builder()
                    .field_type(FieldType::Int)
                    .description("haha"),
            )
            .field(
                "mypath",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description("hehe"),
            )
            .operation(Operation::Read, |backend, req| {
                Box::pin(my_test_read_handler(backend, req))
            })
            .operation(Operation::Write, |_backend, _req| {
                Box::pin(async move { Err(RvError::ErrUnknown) })
            })
            .help("testhelp")
            .build();

        assert_eq!(&path.pattern, "/aa");
        assert_eq!(&path.help, "testhelp");
        assert!(path.fields.get("mytype").is_some());
        assert_eq!(path.fields["mytype"].field_type, FieldType::Int);
        assert_eq!(path.fields["mytype"].description, "haha");
        assert!(path.fields.get("mypath").is_some());
        assert_eq!(path.fields["mypath"].field_type, FieldType::Str);
        assert_eq!(path.fields["mypath"].description, "hehe");
        assert!(path.fields.get("xxfield").is_none());
        assert_eq!(path.operations[0].op, Operation::Read);
        assert_eq!(path.operations[1].op, Operation::Write);
        assert_eq!(path.operations.len(), 2);
    }

    #[test]
    #[cfg(feature = "sync_handler")]
    fn test_logical_path() {
        let path = Path::builder()
            .pattern("/aa")
            .field(
                "mytype",
                Field::builder()
                    .field_type(FieldType::Int)
                    .description("haha"),
            )
            .field(
                "mypath",
                Field::builder()
                    .field_type(FieldType::Str)
                    .description("hehe"),
            )
            .operation(Operation::Read, my_test_read_handler)
            .operation(Operation::Write, |_backend, _req| Err(RvError::ErrUnknown))
            .help("testhelp")
            .build();

        assert_eq!(&path.pattern, "/aa");
        assert_eq!(&path.help, "testhelp");
        assert!(path.fields.get("mytype").is_some());
        assert_eq!(path.fields["mytype"].field_type, FieldType::Int);
        assert_eq!(path.fields["mytype"].description, "haha");
        assert!(path.fields.get("mypath").is_some());
        assert_eq!(path.fields["mypath"].field_type, FieldType::Str);
        assert_eq!(path.fields["mypath"].description, "hehe");
        assert!(path.fields.get("xxfield").is_none());
        assert_eq!(path.operations[0].op, Operation::Read);
        assert_eq!(path.operations[1].op, Operation::Write);
        assert_eq!(path.operations.len(), 2);
    }
}
