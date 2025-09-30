//! The `rusty_vault::metrics` module instruments RustyVault with Prometheus, allowing it to capture performance metrics.
//!
//! # Methodology
//!
//! From a monitoring perspective, [Prometheus](https://prometheus.io/docs/practices/instrumentation/#the-three-types-of-services) categorizes services into three types: online services, offline processing, and batch jobs. As a modern key management system, RustyVault provides a set of RESTful APIs, so it is classified as an online service.
//!
//! In online service systems, the key metrics include the number of executed queries, error rates, and latency. In this project, the monitored content is divided into two parts: the target operating system and the target application service.
//!
//! Based on the [USE (Utilization, Saturation, and Errors) method](https://www.brendangregg.com/usemethod.html), system performance metrics such as CPU, memory, disk, network, and load are monitored. For the target service, the [RED (Rate, Errors, and Duration)](https://grafana.com/blog/2018/08/02/the-red-method-how-to-instrument-your-services/) method is used to monitor the number of requests, request outcomes, and the time taken to process each request.
//!
//! # Dependency
//!
//! This implementation utilizes the [prometheus-client](https://docs.rs/prometheus-client/latest/prometheus_client/) and [sysinfo](https://docs.rs/sysinfo/latest/sysinfo/) libraries to gather system performance data.
//!
//! # How to Create and Using New Metric
//!
//! 1. **Define and Implement Metrics**
//!
//! Define your metrics under `src/metrics/` and register them with the `Registry` like this:
//!
//! ```text
//! pub const HTTP_REQUEST_COUNT: &str = "http_request_count";
//! pub const HTTP_REQUEST_COUNT_HELP: &str = "Number of HTTP requests received, labeled by method and status";
//!
//! pub struct HttpMetrics {
//!     requests: Family<HttpLabel, Counter>,
//! }
//!
//! impl HttpMetrics {
//!     pub fn new(registry: &mut Registry) -> Self {
//!        let requests = Family::<HttpLabel, Counter>::default();
//!        registry.register(HTTP_REQUEST_COUNT, HTTP_REQUEST_COUNT_HELP, requests.clone());
//!         Self { requests }
//!       }
//!
//!       pub fn increment_request_count(&self, label: &HttpLabel) {
//!           self.requests.get_or_create(label).inc();
//!       }
//!   }
//!   ```
//!
//! 2. **Add Metrics to `MetricsManager`**
//!
//! Register the metrics within the `MetricsManager` struct:
//!
//! ```text
//! pub struct MetricsManager {
//!     pub registry: Arc<Mutex<Registry>>,
//!     pub http_metrics: Arc<HttpMetrics>,
//!     // Other fields...
//! }
//!
//! impl MetricsManager {
//!     pub fn new(collection_interval: u64) -> Self {
//!         let registry = Arc::new(Mutex::new(Registry::default()));
//!         let http_metrics = Arc::new(HttpMetrics::new(&mut registry.lock().unwrap()));
//!         MetricsManager { registry, http_metrics }
//!     }
//! }
//! ```
//!
//! 3. **Update Metrics Based on Events**
//!
//! Invoke methods to update metrics where relevant events occur. In this example, retrieve `MetricsManager` from the `app_data` in the Actix Web application:
//!
//! ```text
//! if let Some(m) = res.request().app_data::<Data<Arc<RwLock<MetricsManager>>>>() {
//!     let metrics_manager = m.read().unwrap();
//!     metrics_manager.http_metrics.increment_request_count(&label);
//! }
//! ```
pub mod http_metrics;
pub mod manager;
pub mod middleware;
pub mod system_metrics;
