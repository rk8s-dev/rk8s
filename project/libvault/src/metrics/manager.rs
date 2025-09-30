//! `MetricManager` holds the Prometheus registry and metrics.
use std::sync::{Arc, Mutex};

use prometheus_client::registry::Registry;

use crate::metrics::{http_metrics::HttpMetrics, system_metrics::SystemMetrics};

#[derive(Clone)]
pub struct MetricsManager {
    pub registry: Arc<Mutex<Registry>>,
    pub system_metrics: Arc<SystemMetrics>,
    pub http_metrics: Arc<HttpMetrics>,
}

impl MetricsManager {
    pub fn new(collection_interval: u64) -> Self {
        let registry = Arc::new(Mutex::new(Registry::default()));
        let system_metrics = Arc::new(SystemMetrics::new(&mut registry.lock().unwrap(), collection_interval));
        let http_metrics = Arc::new(HttpMetrics::new(&mut registry.lock().unwrap()));
        MetricsManager { registry, system_metrics, http_metrics }
    }
}
