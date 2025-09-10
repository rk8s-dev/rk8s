pub mod cni;
pub mod libcni;
pub mod namespace;

pub fn is_debug_logging() -> bool {
    log::log_enabled!(log::Level::Debug)
}

#[cfg(test)]
pub mod test {
    use super::*;
    use crate::cni;
    use env_logger::Env;
    use log::{error, info};
    #[test]
    fn test_cni_add_remove() {
        env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
        info!("Starting CNI test");

        let mut cni = cni::Libcni::default();
        cni.load_default_conf();

        let pid = std::process::id();
        let path = format!("/proc/{}/ns/net", pid);
        let id = "test".to_string();

        info!("Setting up network for container {}", id);
        let setup_result = cni.setup(id.clone(), path.clone());
        match &setup_result {
            Ok(_) => info!("Network setup successful"),
            Err(e) => error!("Network setup failed: {}", e),
        }

        info!("Removing network for container {}", id);
        let remove_result = cni.remove(id.clone(), path.clone());
        match &remove_result {
            Ok(_) => info!("Network removal successful"),
            Err(e) => error!("Network removal failed: {}", e),
        }
    }
}
