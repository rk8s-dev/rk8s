/// Operation context for filesystem operations.
/// This provides a way to pass additional parameters beyond the FUSE Request,
/// allowing internal operations to override UID/GID or other parameters.
use rfuse3::raw::Request;

#[derive(Debug, Clone, Copy)]
pub struct OperationContext {
    /// The original FUSE request
    pub req: Request,
    /// Override UID for internal operations, otherwise use req.uid
    pub uid: Option<u32>,
    /// Override GID for internal operations, otherwise use req.gid
    pub gid: Option<u32>,
}

impl From<Request> for OperationContext {
    fn from(req: Request) -> Self {
        OperationContext {
            req,
            uid: None,
            gid: None,
        }
    }
}

impl OperationContext {
    /// Create a new context from a request
    pub fn new(req: Request) -> Self {
        Self::from(req)
    }

    /// Create a context with explicit UID/GID override
    pub fn with_credentials(req: Request, uid: u32, gid: u32) -> Self {
        OperationContext {
            req,
            uid: Some(uid),
            gid: Some(gid),
        }
    }

    /// Get the effective UID (override or from request)
    pub fn effective_uid(&self) -> Option<u32> {
        self.uid.or(Some(self.req.uid))
    }

    /// Get the effective GID (override or from request)
    pub fn effective_gid(&self) -> Option<u32> {
        self.gid.or(Some(self.req.gid))
    }
}
