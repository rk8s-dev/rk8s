use crate::channel::Channel;
use tonic::{
    metadata::AsciiMetadataValue,
    service::{interceptor::InterceptedService, Interceptor as TonicInterceptor},
};

const REQUIRE_LEADER_KEY: &str = "hasleader";
const REQUIRE_LEADER_VALUE: &str = "true";

/// An interceptor that conditionally attaches a leader requirement
/// to the underlying service.
#[derive(Clone)]
pub struct Interceptor {
    pub require_leader: bool,
}

impl TonicInterceptor for Interceptor {
    fn call(
        &mut self,
        mut request: tonic::Request<()>,
    ) -> Result<tonic::Request<()>, tonic::Status> {
        if self.require_leader {
            request.metadata_mut().append(
                REQUIRE_LEADER_KEY,
                AsciiMetadataValue::from_static(REQUIRE_LEADER_VALUE),
            );
        }
        Ok(request)
    }
}

/// A type alias to make using this interceptor easier.
pub type InterceptedChannel = InterceptedService<Channel, Interceptor>;
