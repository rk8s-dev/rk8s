#![allow(dead_code)]
use anyhow::Result;
use ipnetwork::IpNetwork;
use libcni::ip::route::Route;
use log::{error, info, warn};
use std::sync::Arc;

use libnetwork::route::RouteManager;

/// Route receiver for handling route configurations from rks
/// This will be called when receiving route updates from rks
pub struct RouteReceiver {
    route_manager: Arc<tokio::sync::Mutex<RouteManager>>,
}

impl RouteReceiver {
    pub fn new(route_manager: Arc<tokio::sync::Mutex<RouteManager>>) -> Self {
        Self { route_manager }
    }

    /// Handle received route configuration from rks
    /// This function will be called when rks sends route configuration
    pub async fn handle_route_config(&self, routes: Vec<Route>) -> Result<()> {
        info!("Received {} route configurations from rks", routes.len());

        let mut manager = self.route_manager.lock().await;

        for route in routes {
            match &route.dst {
                Some(IpNetwork::V4(_)) => {
                    if let Err(e) = manager.add_route(&route).await {
                        error!("Failed to add IPv4 route {route:?}: {e}");
                    } else {
                        info!("Successfully added IPv4 route: {route:?}");
                    }
                }
                Some(IpNetwork::V6(_)) => {
                    if let Err(e) = manager.add_route(&route).await {
                        error!("Failed to add IPv6 route {route:?}: {e}");
                    } else {
                        info!("Successfully added IPv6 route: {route:?}");
                    }
                }
                None => {
                    warn!("Skipping route with no destination: {route:?}");
                }
            }
        }

        info!("Route configuration applied successfully");
        Ok(())
    }

    /// Receive route configurations from rks via QUIC
    /// This method will be called by the main NetworkReceiver QUIC loop
    pub async fn receive_from_rks(&self) -> Result<()> {
        // This method is now integrated with the main NetworkReceiver QUIC communication
        // The actual QUIC communication is handled by NetworkReceiver.quic_communication_loop()
        // When route configurations are received, they will be passed to handle_route_config()
        info!("Route receiver is ready to receive configurations from rks");
        Ok(())
    }

    /// Remove specific routes
    pub async fn remove_routes(&self, routes: Vec<Route>) -> Result<()> {
        info!("Removing {} route configurations", routes.len());

        let manager = self.route_manager.lock().await;

        for route in routes {
            if let Err(e) = manager.delete_route(&route).await {
                error!("Failed to remove route {route:?}: {e}");
            } else {
                info!("Successfully removed route: {route:?}");
            }
        }

        info!("Route removal completed");
        Ok(())
    }
}
