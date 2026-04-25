use opentelemetry::{
    global,
    propagation::{Extractor, Injector},
};
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use xlinerpc::MetaData;

/// Struct for extract data from `MetaData`
struct ExtractMap<'a>(&'a MetaData);

impl Extractor for ExtractMap<'_> {
    /// Get a value for a key from the `MetaData`. If the value is not UTF-8, returns `None`.
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get_str(key).and_then(Result::ok)
    }

    /// Collect all UTF-8 keys from the `MetaData`.
    fn keys(&self) -> Vec<&str> {
        self.0
            .iter()
            .filter_map(|(key, _)| std::str::from_utf8(key).ok())
            .collect::<Vec<_>>()
    }
}

/// Function for extract data from some struct
pub trait Extract {
    /// extract span context from self and set as parent context
    fn extract_span(&self);
}

impl Extract for MetaData {
    #[inline]
    fn extract_span(&self) {
        let parent_ctx = global::get_text_map_propagator(|prop| prop.extract(&ExtractMap(self)));
        let span = Span::current();
        span.set_parent(parent_ctx);
    }
}

/// Struct for inject data to `MetaData`
struct InjectMap<'a>(&'a mut MetaData);

impl Injector for InjectMap<'_> {
    /// Set a key and value in the `MetaData`.
    fn set(&mut self, key: &str, value: String) {
        self.0.insert(key, value);
    }
}

/// Function for inject data to some struct
pub trait Inject {
    /// Inject span context into self
    fn inject_span(&mut self, span: &Span);

    /// Inject span context into self
    #[inline]
    fn inject_current(&mut self) {
        let curr_span = Span::current();
        self.inject_span(&curr_span);
    }
}

impl Inject for MetaData {
    #[inline]
    fn inject_span(&mut self, span: &Span) {
        let ctx = span.context();
        global::get_text_map_propagator(|prop| {
            prop.inject_context(&ctx, &mut InjectMap(self));
        });
    }
}

#[cfg(test)]
mod test {

    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry::trace::{TraceContextExt, TraceId};
    use opentelemetry_sdk::propagation::TraceContextPropagator;
    use tracing::info_span;
    use tracing_subscriber::{
        prelude::__tracing_subscriber_SubscriberExt, util::SubscriberInitExt,
    };

    use super::*;
    #[tokio::test(flavor = "multi_thread")]
    #[allow(clippy::unwrap_in_result)]
    async fn test_inject_and_extract() -> Result<(), Box<dyn std::error::Error>> {
        init()?;
        global::set_text_map_propagator(TraceContextPropagator::new());
        let span = info_span!("test span");
        let _entered = span.enter();
        let outer_trace_id = span.context().span().span_context().trace_id();
        let mut request = xlinerpc::Request::from_data("message");
        request.metadata_mut().inject_current();
        let inner_trace_id = inner_fun(&request);
        assert_eq!(outer_trace_id, inner_trace_id);
        Ok(())
    }

    fn inner_fun(request: &xlinerpc::Request<&str>) -> TraceId {
        let span = info_span!("inner span");
        let _entered = span.enter();
        request.metadata().extract_span();
        Span::current().context().span().span_context().trace_id()
    }

    /// init tracing subscriber
    fn init() -> Result<(), Box<dyn std::error::Error>> {
        let otlp_exporter = opentelemetry_otlp::new_exporter().http();
        let provider = opentelemetry_otlp::new_pipeline()
            .tracing()
            .with_exporter(otlp_exporter)
            .install_batch(opentelemetry_sdk::runtime::Tokio)?;
        global::set_tracer_provider(provider.clone());
        let tracer = provider.tracer("xline");
        let jaeger_online_layer = tracing_opentelemetry::layer().with_tracer(tracer);
        tracing_subscriber::registry()
            .with(jaeger_online_layer)
            .init();
        Ok(())
    }
}
