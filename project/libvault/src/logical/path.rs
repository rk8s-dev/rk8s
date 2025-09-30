#[cfg(not(feature = "sync_handler"))]
use std::future::Future;
#[cfg(not(feature = "sync_handler"))]
use std::pin::Pin;
use std::{collections::HashMap, fmt, sync::Arc};

use super::{request::Request, response::Response, Backend, Field, Operation};
use crate::{context::Context, errors::RvError};

#[cfg(not(feature = "sync_handler"))]
type PathOperationHandler = dyn for<'a> Fn(
        &'a dyn Backend,
        &'a mut Request,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Response>, RvError>> + Send + 'a>>
    + Send
    + Sync;
#[cfg(feature = "sync_handler")]
type PathOperationHandler = dyn Fn(&dyn Backend, &mut Request) -> Result<Option<Response>, RvError> + Send + Sync;

#[derive(Debug, Clone)]
pub struct Path {
    pub ctx: Arc<Context>,
    pub pattern: String,
    pub fields: HashMap<String, Arc<Field>>,
    pub operations: Vec<PathOperation>,
    pub help: String,
}

#[derive(Clone)]
pub struct PathOperation {
    pub op: Operation,
    pub handler: Arc<PathOperationHandler>,
}

impl fmt::Debug for PathOperation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PathOperation").field("op", &self.op).finish()
    }
}

impl Path {
    pub fn new(pattern: &str) -> Self {
        Self {
            ctx: Arc::new(Context::new()),
            pattern: pattern.to_string(),
            fields: HashMap::new(),
            operations: Vec::new(),
            help: String::new(),
        }
    }

    pub fn get_field(&self, key: &str) -> Option<Arc<Field>> {
        self.fields.get(key).cloned()
    }
}

#[maybe_async::maybe_async]
impl PathOperation {
    #[cfg(not(feature = "sync_handler"))]
    pub fn new() -> Self {
        Self { op: Operation::Read, handler: Arc::new(|_backend, _req| Box::pin(async move { Ok(None) })) }
    }
    #[cfg(feature = "sync_handler")]
    pub fn new() -> Self {
        Self { op: Operation::Read, handler: Arc::new(|_backend, _req| Ok(None)) }
    }

    pub async fn handle_request(&self, backend: &dyn Backend, req: &mut Request) -> Result<Option<Response>, RvError> {
        (self.handler)(backend, req).await
    }
}

#[macro_export]
macro_rules! new_fields {
    ($($tt:tt)*) => {
        new_fields_internal!($($tt)*)
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! new_fields_internal {
    (@object $object:ident field_type: $field_type:expr) => {
        $object.field_type = $field_type;
    };
    (@object $object:ident required: $required:expr) => {
        $object.required = $required;
    };
    (@object $object:ident default: $default:expr) => {
        let val = serde_json::json!($default);
        $object.default = val;
        $object.required = false;
    };
    (@object $object:ident description: $description:expr) => {
        $object.description = $description.to_string();
    };
    (@object $object:ident field_tt: {$($key:ident: $value:expr),*}) => {
        {
            let mut field = Field::new();

            $(
                new_fields_internal!(@object field $key: $value);
            )*

            field
        }
    };
    (@object $object:ident $($field_name:tt: $field_tt:tt),*) => {
        $(
            $object.insert($field_name.to_string(),
                           Arc::new(new_fields_internal!(@object $object field_tt: $field_tt)));
        )*
    };
    ({ $($tt:tt)+ }) => {
        {
            let mut fields = HashMap::new();
            new_fields_internal!(@object fields $($tt)+);
            fields
        }
    };
}

#[macro_export]
macro_rules! new_path {
    ($($tt:tt)*) => {
        new_path_internal!($($tt)*)
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! new_path_internal {
    (@object $object:ident pattern: $pattern:expr) => {
        $object.pattern = $pattern.to_string();
    };
    (@object $object:ident help: $help:expr) => {
        $object.help = $help.to_string();
    };
    (@object $object:ident
        fields: {
            $($tt:tt)+
        }
    ) => {
        $object.fields = new_fields!({ $($tt)+ });
    };
    (@object $object:ident op: $op:expr) => {
        $object.op = $op;
    };
    (@operation $operation:ident []) => {
    };
    (@operation $object:ident [{op: $op:expr, handler: $handler_obj:ident$(.$handler_method:ident)*} $($rest:tt)*]) => {
        let mut path_op = PathOperation::new();

        path_op.op = $op;
        path_op.handler = Arc::new(move |backend, req| {
            let self_ = $handler_obj.clone();
            #[cfg(not(feature = "sync_handler"))]
            {
                Box::pin(async move {
                    self_$(.$handler_method)*(backend, req).await
                })
            }
            #[cfg(feature = "sync_handler")]
            self_$(.$handler_method)*(backend, req)
        });

        $object.operations.push(path_op);

        new_path_internal!(@operation $object [$($rest)*]);
    };
    (@operation $object:ident [{op: $op:expr, raw_handler: $handler:expr} $($rest:tt)*]) => {
        let mut path_op = PathOperation::new();

        path_op.op = $op;
        path_op.handler = Arc::new(|backend, req| {
            #[cfg(not(feature = "sync_handler"))]
            {
                Box::pin(async move {
                    $handler(backend, req).await
                })
            }
            #[cfg(feature = "sync_handler")]
            $handler(backend, req)
        });

        $object.operations.push(path_op);

        new_path_internal!(@operation $object [$($rest)*]);
    };
    (@object $object:ident
        operations: [
            $($operation:tt),* $(,)?
        ]
    ) => {
        new_path_internal!(@operation $object [$($operation)*]);
    };
    (@object $object:ident () $($key:ident: $value:tt),*) => {
        $(
            new_path_internal!(@object $object $key: $value);
        )*
    };
    ({ $($tt:tt)+ }) => {
        {
            let mut path = Path {
                ctx: Arc::new(Context::new()),
                pattern: String::new(),
                fields: HashMap::new(),
                operations: Vec::new(),
                help: String::new(),
            };
            new_path_internal!(@object path () $($tt)+);
            path
        }
    };
}

#[cfg(test)]
mod test {
    use super::{super::FieldType, *};

    #[maybe_async::maybe_async]
    pub async fn my_test_read_handler(_backend: &dyn Backend, _req: &mut Request) -> Result<Option<Response>, RvError> {
        Ok(None)
    }

    #[test]
    #[cfg(not(feature = "sync_handler"))]
    fn test_logical_path() {
        let path: Path = new_path!({
            pattern: "/aa",
            fields: {
                "mytype": {
                    field_type: FieldType::Int,
                    description: "haha"
                },
                "mypath": {
                    field_type: FieldType::Str,
                    description: "hehe"
                }
            },
            operations: [
                {op: Operation::Read, handler: my_test_read_handler},
                {op: Operation::Write, raw_handler: async move |_backend: &dyn Backend, _req: &mut Request| -> Result<Option<Response>, RvError> {
                        Err(RvError::ErrUnknown)
                    }
                }
            ],
            help: "testhelp"
        });

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
        let path: Path = new_path!({
            pattern: "/aa",
            fields: {
                "mytype": {
                    field_type: FieldType::Int,
                    description: "haha"
                },
                "mypath": {
                    field_type: FieldType::Str,
                    description: "hehe"
                }
            },
            operations: [
                {op: Operation::Read, handler: my_test_read_handler},
                {op: Operation::Write, raw_handler: |_backend: &dyn Backend, _req: &mut Request| -> Result<Option<Response>, RvError> {
                        Err(RvError::ErrUnknown)
                    }
                }
            ],
            help: "testhelp"
        });

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
