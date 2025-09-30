use std::sync::{Arc, RwLock};

use actix_web::{web, HttpResponse};
use prometheus_client::encoding::text::encode;

use crate::metrics::manager::MetricsManager;

pub async fn metrics_handler(metrics_manager: web::Data<Arc<RwLock<MetricsManager>>>) -> HttpResponse {
    let m = metrics_manager.read().unwrap();
    let registry = m.registry.lock().unwrap();

    let mut buffer = String::new();
    if let Err(e) = encode(&mut buffer, &registry) {
        log::error!("Failed to encode metrics: {e}");
        return HttpResponse::InternalServerError().finish();
    }

    HttpResponse::Ok().content_type("text/plain; version=0.0.4").body(buffer)
}

pub fn init_metrics_service(cfg: &mut web::ServiceConfig) {
    cfg.service(web::resource("/metrics").route(web::get().to(metrics_handler)));
}
