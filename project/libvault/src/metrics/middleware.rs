//! Actix-web middleware function, captures and monitors HTTP requests.
//!
//! # Usage
//! The actix-web middleware function could be used as following:
//!
//! ```text
//! let mut http_server = HttpServer::new(move || {
//!        App::new()
//!            //skip
//!            .wrap(from_fn(metrics_midleware))
//!            //skip
//!    })
//! ```
use std::{
    sync::{Arc, RwLock},
    time::Instant,
};

use actix_web::{
    body::MessageBody,
    dev::{ServiceRequest, ServiceResponse},
    http::Method,
    middleware::Next,
    web::Data,
    Error,
};

use super::{http_metrics::MetricsMethod, manager::MetricsManager};
use crate::metrics::http_metrics::HttpLabel;

pub async fn metrics_midleware(
    req: ServiceRequest,
    next: Next<impl MessageBody>,
) -> Result<ServiceResponse<impl MessageBody>, Error> {
    let start_time = Instant::now();
    let path = req.path().to_string();
    let method = match *req.method() {
        Method::GET => MetricsMethod::GET,
        _ if *req.method() == "LIST" => MetricsMethod::LIST,
        Method::POST => MetricsMethod::POST,
        Method::PUT => MetricsMethod::PUT,
        Method::DELETE => MetricsMethod::DELETE,
        _ => MetricsMethod::OTHER,
    };

    let res = next.call(req).await?;

    let status = res.status().as_u16();
    let label = HttpLabel { path, method, status };
    if let Some(m) = res.request().app_data::<Data<Arc<RwLock<MetricsManager>>>>() {
        let metrics_manager = m.read().unwrap();
        metrics_manager.http_metrics.increment_request_count(&label);
        let duration = start_time.elapsed().as_secs_f64();
        metrics_manager.http_metrics.observe_duration(&label, duration);
    }

    Ok(res)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::{
        metrics::{http_metrics::*, system_metrics::*},
        test_utils::TestHttpServer,
    };

    static SYS_METRICS_MAP: &[(&str, &str)] = &[
        (CPU_USAGE_PERCENT, CPU_USAGE_PERCENT_HELP),
        (TOTAL_MEMORY, TOTAL_MEMORY_HELP),
        (USED_MEMORY, USED_MEMORY_HELP),
        (FREE_MEMORY, FREE_MEMORY_HELP),
        (TOTAL_DISK_SPACE, TOTAL_DISK_SPACE_HELP),
        (TOTAL_DISK_AVAILABLE, TOTAL_DISK_AVAILABLE_HELP),
        // (NETWORK_IN, NETWORK_IN_HELP),
        // (NETWORK_OUT, NETWORK_OUT_HELP),
        (LOAD_AVERAGE, LOAD_AVERAGE_HELP),
    ];

    static HTTP_METRICS_MAP: &[(&str, &str)] = &[
        (HTTP_REQUEST_COUNT, HTTP_REQUEST_COUNT_HELP),
        (HTTP_REQUEST_DURATION_SECONDS, HTTP_REQUEST_DURATION_SECONDS_HELP),
    ];

    fn parse_metrics_name_help(raw: &str) -> HashMap<String, String> {
        let mut metrics_map = HashMap::new();
        for line in raw.split('\n') {
            if line.starts_with("# HELP") {
                let line = line.trim_end_matches(".");
                // # PROPERTY METRIC_NAME METRIC_HELP
                // # HELP cpu_usage_percent CPU usage percent.
                let parts: Vec<&str> = line.split(" ").collect();
                let metric_name = parts[2].to_string();
                let metric_help = parts[3..].join(" ");
                metrics_map.insert(metric_name, metric_help);
            }
        }
        metrics_map
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_metrics_name_and_help_info() {
        let sys_metrics_map: HashMap<&str, &str> = SYS_METRICS_MAP.iter().cloned().collect();
        let http_metrics_map: HashMap<&str, &str> = HTTP_METRICS_MAP.iter().cloned().collect();

        let server = TestHttpServer::new_with_prometheus("test_metrics_name_and_help_info", false).await;
        let root_token = &server.root_token;
        let (status, resp) = server.request_prometheus("GET", "metrics", None, Some(root_token), None).unwrap();
        assert_eq!(status, 200);

        let metrics_map = parse_metrics_name_help(resp["metrics"].as_str().unwrap());
        assert_eq!(sys_metrics_map.len() + http_metrics_map.len(), metrics_map.len());

        for (metric_name, metric_help) in &metrics_map {
            let name = metric_name.as_str();
            let help = metric_help.as_str();
            if sys_metrics_map.contains_key(name) {
                assert_eq!(sys_metrics_map.get(name), Some(&help));
            } else if http_metrics_map.contains_key(name) {
                assert_eq!(http_metrics_map.get(name), Some(&help));
            }
        }
    }
}
