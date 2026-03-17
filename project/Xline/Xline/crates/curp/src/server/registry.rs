use std::collections::HashMap;
use std::sync::Arc;

use super::CurpServer;

/// Curp service registry
/// Used to manage and register different CurpServer instances
pub struct CurpServiceRegistry {
    /// 服务映射表
    services: HashMap<String, Arc<dyn CurpServer>>,
}

impl CurpServiceRegistry {
    /// Create a new service registry
    pub fn new() -> Self {
        Self {
            services: HashMap::new(),
        }
    }
    
    /// Register service
    pub fn register_service(&mut self, name: String, service: Arc<dyn CurpServer>) {
        self.services.insert(name, service);
    }
    
    /// Get service
    pub fn get_service(&self, name: &str) -> Option<Arc<dyn CurpServer>> {
        self.services.get(name).cloned()
    }
    
    /// Remove service
    pub fn remove_service(&mut self, name: &str) -> Option<Arc<dyn CurpServer>> {
        self.services.remove(name)
    }
    
    /// Check if service exists
    pub fn has_service(&self, name: &str) -> bool {
        self.services.contains_key(name)
    }
    
    /// Get all service names
    pub fn get_service_names(&self) -> Vec<String> {
        self.services.keys().cloned().collect()
    }
}

impl Default for CurpServiceRegistry {
    fn default() -> Self {
        Self::new()
    }
}