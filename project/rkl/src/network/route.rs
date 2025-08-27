use anyhow::Result;
use ipnetwork::{IpNetwork, Ipv4Network, Ipv6Network};
use log::{debug, error, info, warn};
use std::{net::IpAddr, sync::Arc};
use tokio::{
    select,
    sync::mpsc,
    time::{Duration, sleep},
};

// Platform-specific imports for actual route manipulation
#[cfg(target_os = "linux")]
use {
    netlink_packet_core::{NetlinkMessage, NLM_F_ACK, NLM_F_CREATE, NLM_F_EXCL, NLM_F_REQUEST},
    netlink_packet_route::{
        RouteMessage, RtnlMessage, AddressFamily as NetlinkAddressFamily, 
        route::{RouteHeader, RouteType, Nla as RouteNla}
    },
    netlink_sys::{AsyncSocket, SocketAddr as NetlinkSocketAddr, protocols::NETLINK_ROUTE},
    futures::stream::StreamExt,
};

// Custom enums for cross-platform compatibility
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AddressFamily {
    Inet,
    Inet6,
    Unspec,
}

impl From<AddressFamily> for u8 {
    fn from(family: AddressFamily) -> u8 {
        match family {
            AddressFamily::Inet => 2,    // AF_INET
            AddressFamily::Inet6 => 10,  // AF_INET6  
            AddressFamily::Unspec => 0,  // AF_UNSPEC
        }
    }
}

#[cfg(target_os = "linux")]
impl From<AddressFamily> for NetlinkAddressFamily {
    fn from(family: AddressFamily) -> NetlinkAddressFamily {
        match family {
            AddressFamily::Inet => NetlinkAddressFamily::Inet,
            AddressFamily::Inet6 => NetlinkAddressFamily::Inet6,
            AddressFamily::Unspec => NetlinkAddressFamily::Unspec,
        }
    }
}

/// Lease information for route generation
/// This is a simplified version that will be received from rks
#[derive(Debug, Clone)]
pub struct NetworkLease {
    pub enable_ipv4: bool,
    pub enable_ipv6: bool,
    pub subnet: Ipv4Network,
    pub ipv6_subnet: Option<Ipv6Network>,
    pub public_ip: std::net::Ipv4Addr,
    pub public_ipv6: Option<std::net::Ipv6Addr>,
}

/// Route configuration structure received from rks
#[derive(Debug, Clone)]
pub struct RouteConfig {
    pub destination: IpNetwork,
    pub gateway: Option<IpAddr>,
    pub interface_index: Option<u32>,
    pub metric: Option<u32>,
}

/// Route manager for handling system routing table operations on rkl node
pub struct RouteManager {
    link_index: u32,
    backend_type: String,
    routes: Vec<RouteConfig>,
    v6routes: Vec<RouteConfig>,
}

pub trait RouteListOps {
    fn add_to_route_list(&mut self, route: RouteConfig, family: AddressFamily);
    fn remove_from_route_list(&mut self, route: &RouteConfig, family: AddressFamily);
}

impl RouteListOps for RouteManager {
    fn add_to_route_list(&mut self, route: RouteConfig, family: AddressFamily) {
        match family {
            AddressFamily::Inet => {
                self.routes = add_to_route_list(route, std::mem::take(&mut self.routes));
            }
            AddressFamily::Inet6 => {
                self.v6routes = add_to_route_list(route, std::mem::take(&mut self.v6routes));
            }
            _ => {}
        }
    }

    fn remove_from_route_list(&mut self, route: &RouteConfig, family: AddressFamily) {
        match family {
            AddressFamily::Inet => {
                self.routes = remove_from_route_list(route, &self.routes);
            }
            AddressFamily::Inet6 => {
                self.v6routes = remove_from_route_list(route, &self.v6routes);
            }
            _ => {}
        }
    }
}

impl RouteManager {
    pub fn new(link_index: u32, backend_type: String) -> Self {
        let routes: Vec<RouteConfig> = vec![];
        let v6routes: Vec<RouteConfig> = vec![];
        Self {
            link_index,
            backend_type,
            routes,
            v6routes,
        }
    }

    /// Generate IPv4 route for a lease
    pub fn get_route_for_lease(&self, lease: &NetworkLease) -> Option<RouteConfig> {
        if !lease.enable_ipv4 {
            return None;
        }

        Some(RouteConfig {
            destination: IpNetwork::V4(lease.subnet),
            gateway: Some(IpAddr::V4(lease.public_ip)),
            interface_index: Some(self.link_index),
            metric: None,
        })
    }

    /// Generate IPv6 route for a lease
    pub fn get_v6_route_for_lease(&self, lease: &NetworkLease) -> Option<RouteConfig> {
        if !lease.enable_ipv6 {
            return None;
        }

        let subnet_v6 = lease.ipv6_subnet?;
        let gateway_v6 = lease.public_ipv6?;

        Some(RouteConfig {
            destination: IpNetwork::V6(subnet_v6),
            gateway: Some(IpAddr::V6(gateway_v6)),
            interface_index: Some(self.link_index),
            metric: None,
        })
    }

    /// Add a route to the system routing table
    pub async fn add_route(&mut self, route: &RouteConfig) -> Result<()> {
        info!(
            "Adding route: {:?} via {:?} dev {:?} ({})",
            route.destination, route.gateway, route.interface_index, self.backend_type
        );
        
        let family = match route.destination {
            IpNetwork::V4(_) => AddressFamily::Inet,
            IpNetwork::V6(_) => AddressFamily::Inet6,
        };

        // Perform actual route addition
        match self.add_system_route(route).await {
            Ok(_) => {
                info!("Successfully added route: {:?}", route);
                self.add_to_route_list(route.clone(), family);
            }
            Err(e) => {
                error!("Failed to add route {:?}: {}", route, e);
                return Err(e);
            }
        }
        
        Ok(())
    }

    /// Add an IPv6 route to the system routing table
    pub async fn add_v6_route(&mut self, route: &RouteConfig) -> Result<()> {
        // IPv6 routes are now handled by the unified add_route method
        self.add_route(route).await
    }

    /// Remove a route from the system routing table
    pub async fn delete_route(&mut self, route: &RouteConfig) -> Result<()> {
        info!(
            "Removing route: {:?} via {:?} dev {:?} ({})",
            route.destination, route.gateway, route.interface_index, self.backend_type
        );
        
        let family = match route.destination {
            IpNetwork::V4(_) => AddressFamily::Inet,
            IpNetwork::V6(_) => AddressFamily::Inet6,
        };

        // Perform actual route deletion
        match self.delete_system_route(route).await {
            Ok(_) => {
                info!("Successfully deleted route: {:?}", route);
                self.remove_from_route_list(route, family);
            }
            Err(e) => {
                error!("Failed to delete route {:?}: {}", route, e);
                return Err(e);
            }
        }
        
        Ok(())
    }

    /// Synchronize routes with a list of leases
    pub async fn sync_routes(&mut self, leases: &[NetworkLease]) -> Result<()> {
        debug!("Synchronizing routes for {} leases", leases.len());

        for lease in leases {
            if let Some(route) = self.get_route_for_lease(lease) {
                if let Err(e) = self.add_route(&route).await {
                    warn!("Failed to add IPv4 route for lease {}: {}", lease.subnet, e);
                }
            }

            if let Some(route_v6) = self.get_v6_route_for_lease(lease) {
                if let Err(e) = self.add_v6_route(&route_v6).await {
                    warn!(
                        "Failed to add IPv6 route for lease {:?}: {}",
                        lease.ipv6_subnet, e
                    );
                }
            }
        }
        Ok(())
    }

    /// Clean up all routes for a list of leases
    pub async fn cleanup_routes(&mut self, leases: &[NetworkLease]) -> Result<()> {
        debug!("Cleaning up routes for {} leases", leases.len());

        for lease in leases {
            if let Some(route) = self.get_route_for_lease(lease) {
                if let Err(e) = self.delete_route(&route).await {
                    warn!(
                        "Failed to remove IPv4 route for lease {}: {}",
                        lease.subnet, e
                    );
                }
            }

            if let Some(route_v6) = self.get_v6_route_for_lease(lease) {
                if let Err(e) = self.delete_route(&route_v6).await {
                    warn!(
                        "Failed to remove IPv6 route for lease {:?}: {}",
                        lease.ipv6_subnet, e
                    );
                }
            }
        }

        Ok(())
    }

    /// Add route to system routing table using netlink (Linux only)
    #[cfg(target_os = "linux")]
    async fn add_system_route(&self, route: &RouteConfig) -> Result<()> {
        use std::process::Command;
        
        // For now, use 'ip route' command as a fallback
        // In production, this should use the netlink API directly
        let dest_str = route.destination.to_string();
        let mut cmd = Command::new("ip");
        cmd.arg("route").arg("add").arg(&dest_str);
        
        if let Some(gateway) = route.gateway {
            cmd.arg("via").arg(gateway.to_string());
        }
        
        if let Some(interface_index) = route.interface_index {
            // Convert interface index to interface name
            cmd.arg("dev").arg(format!("eth{}", interface_index));
        }
        
        if let Some(metric) = route.metric {
            cmd.arg("metric").arg(metric.to_string());
        }
        
        let output = cmd.output()?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Ignore "File exists" errors as the route might already exist
            if !stderr.contains("File exists") {
                return Err(anyhow::anyhow!("Failed to add route: {}", stderr));
            }
        }
        
        info!("Route added successfully via 'ip route' command");
        Ok(())
    }

    /// Delete route from system routing table using netlink (Linux only)
    #[cfg(target_os = "linux")]
    async fn delete_system_route(&self, route: &RouteConfig) -> Result<()> {
        use std::process::Command;
        
        // For now, use 'ip route' command as a fallback
        // In production, this should use the netlink API directly
        let dest_str = route.destination.to_string();
        let mut cmd = Command::new("ip");
        cmd.arg("route").arg("del").arg(&dest_str);
        
        if let Some(gateway) = route.gateway {
            cmd.arg("via").arg(gateway.to_string());
        }
        
        if let Some(interface_index) = route.interface_index {
            // Convert interface index to interface name
            cmd.arg("dev").arg(format!("eth{}", interface_index));
        }
        
        let output = cmd.output()?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Ignore "No such process" errors as the route might not exist
            if !stderr.contains("No such process") && !stderr.contains("Cannot find device") {
                return Err(anyhow::anyhow!("Failed to delete route: {}", stderr));
            }
        }
        
        info!("Route deleted successfully via 'ip route' command");
        Ok(())
    }

    /// Stub for non-Linux platforms
    #[cfg(not(target_os = "linux"))]
    async fn add_system_route(&self, route: &RouteConfig) -> Result<()> {
        warn!("Route addition not implemented for this platform: {:?}", route);
        Ok(())
    }

    /// Stub for non-Linux platforms
    #[cfg(not(target_os = "linux"))]
    async fn delete_system_route(&self, route: &RouteConfig) -> Result<()> {
        warn!("Route deletion not implemented for this platform: {:?}", route);
        Ok(())
    }

    /// Check if routes exist in the routing table and recover them if missing
    pub async fn check_subnet_exist_in_v4_routes(&self) {
        if let Err(e) = self
            .check_subnet_exist_in_routes(&self.routes, AddressFamily::Inet)
            .await
        {
            error!("Error checking v4 routes: {e:?}");
        }
    }

    pub async fn check_subnet_exist_in_v6_routes(&self) {
        if let Err(e) = self
            .check_subnet_exist_in_routes(&self.v6routes, AddressFamily::Inet6)
            .await
        {
            error!("Error checking v6 routes: {e:?}");
        }
    }

    async fn check_subnet_exist_in_routes(
        &self,
        routes: &[RouteConfig],
        _family: AddressFamily,
    ) -> Result<()> {
        // TODO: Implement actual route list fetching and comparison
        // For now, just log the operation
        for route in routes {
            info!("Checking route: {:?} -> {:?}", route.destination, route.gateway);
        }
        
        warn!("Route checking not yet implemented");
        Ok(())
    }

    /// Periodic route checking task
    pub async fn route_check(
        self: Arc<Self>,
        mut shutdown_rx: mpsc::Receiver<()>,
        interval_secs: u64,
    ) {
        loop {
            select! {
                _ = shutdown_rx.recv() => {
                    info!("Route check task shutting down");
                    break;
                }
                _ = sleep(Duration::from_secs(interval_secs)) => {
                    self.check_subnet_exist_in_v4_routes().await;
                    self.check_subnet_exist_in_v6_routes().await;
                }
            }
        }
    }

    /// Get current routes
    pub fn get_routes(&self) -> &[RouteConfig] {
        &self.routes
    }

    /// Get current IPv6 routes
    pub fn get_v6_routes(&self) -> &[RouteConfig] {
        &self.v6routes
    }
}

pub fn add_to_route_list(route: RouteConfig, mut routes: Vec<RouteConfig>) -> Vec<RouteConfig> {
    for r in &routes {
        if route_equal(r, &route) {
            return routes;
        }
    }
    routes.push(route);
    routes
}

pub fn remove_from_route_list(target: &RouteConfig, routes: &[RouteConfig]) -> Vec<RouteConfig> {
    let mut result = Vec::with_capacity(routes.len());
    let mut removed = false;

    for r in routes {
        if !removed && route_equal(r, target) {
            removed = true;
            continue;
        }
        result.push(r.clone());
    }

    result
}

/// Compare two routes for equality
pub fn route_equal(a: &RouteConfig, b: &RouteConfig) -> bool {
    a.destination == b.destination
        && a.gateway == b.gateway
        && a.interface_index == b.interface_index
        && a.metric == b.metric
}

/// Add blackhole route for IPv4 network
#[cfg(target_os = "linux")]
pub async fn add_blackhole_v4_route(dst: Ipv4Network) -> Result<()> {
    use std::process::Command;
    
    info!("Adding blackhole route for {dst}");
    
    let mut cmd = Command::new("ip");
    cmd.arg("route")
       .arg("add")
       .arg("blackhole")
       .arg(dst.to_string());
    
    let output = cmd.output()?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.contains("File exists") {
            return Err(anyhow::anyhow!("Failed to add blackhole route: {}", stderr));
        }
    }
    
    info!("Blackhole route added successfully for {dst}");
    Ok(())
}

/// Add blackhole route for IPv6 network
#[cfg(target_os = "linux")]
pub async fn add_blackhole_v6_route(dst: Ipv6Network) -> Result<()> {
    use std::process::Command;
    
    info!("Adding blackhole route for {dst}");
    
    let mut cmd = Command::new("ip");
    cmd.arg("-6")
       .arg("route")
       .arg("add")
       .arg("blackhole")
       .arg(dst.to_string());
    
    let output = cmd.output()?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.contains("File exists") {
            return Err(anyhow::anyhow!("Failed to add IPv6 blackhole route: {}", stderr));
        }
    }
    
    info!("IPv6 blackhole route added successfully for {dst}");
    Ok(())
}

/// Stub for non-Linux platforms
#[cfg(not(target_os = "linux"))]
pub async fn add_blackhole_v4_route(dst: Ipv4Network) -> Result<()> {
    warn!("Blackhole route addition not implemented for this platform: {dst}");
    Ok(())
}

/// Stub for non-Linux platforms
#[cfg(not(target_os = "linux"))]
pub async fn add_blackhole_v6_route(dst: Ipv6Network) -> Result<()> {
    warn!("IPv6 blackhole route addition not implemented for this platform: {dst}");
    Ok(())
}

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
    pub async fn handle_route_config(&self, routes: Vec<RouteConfig>) -> Result<()> {
        info!("Received {} route configurations from rks", routes.len());
        
        let mut manager = self.route_manager.lock().await;
        
        for route in routes {
            match route.destination {
                IpNetwork::V4(_) => {
                    if let Err(e) = manager.add_route(&route).await {
                        error!("Failed to add IPv4 route {:?}: {}", route, e);
                    } else {
                        info!("Successfully added IPv4 route: {:?}", route);
                    }
                }
                IpNetwork::V6(_) => {
                    if let Err(e) = manager.add_route(&route).await {
                        error!("Failed to add IPv6 route {:?}: {}", route, e);
                    } else {
                        info!("Successfully added IPv6 route: {:?}", route);
                    }
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
    pub async fn remove_routes(&self, routes: Vec<RouteConfig>) -> Result<()> {
        info!("Removing {} route configurations", routes.len());
        
        let mut manager = self.route_manager.lock().await;
        
        for route in routes {
            if let Err(e) = manager.delete_route(&route).await {
                error!("Failed to remove route {:?}: {}", route, e);
            } else {
                info!("Successfully removed route: {:?}", route);
            }
        }

        info!("Route removal completed");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_route_equal() {
        let route1 = RouteConfig {
            destination: IpNetwork::V4("10.0.1.0/24".parse().unwrap()),
            gateway: Some(IpAddr::V4("192.168.1.1".parse().unwrap())),
            interface_index: Some(1),
            metric: Some(100),
        };

        let route2 = RouteConfig {
            destination: IpNetwork::V4("10.0.1.0/24".parse().unwrap()),
            gateway: Some(IpAddr::V4("192.168.1.1".parse().unwrap())),
            interface_index: Some(1),
            metric: Some(100),
        };

        let route3 = RouteConfig {
            destination: IpNetwork::V4("10.0.2.0/24".parse().unwrap()),
            gateway: Some(IpAddr::V4("192.168.1.1".parse().unwrap())),
            interface_index: Some(1),
            metric: Some(100),
        };

        assert!(route_equal(&route1, &route2));
        assert!(!route_equal(&route1, &route3));
    }

    #[test]
    fn test_add_to_route_list() {
        let route = RouteConfig {
            destination: IpNetwork::V4("10.0.1.0/24".parse().unwrap()),
            gateway: Some(IpAddr::V4("192.168.1.1".parse().unwrap())),
            interface_index: Some(1),
            metric: Some(100),
        };

        let mut routes = vec![];
        routes = add_to_route_list(route.clone(), routes);
        assert_eq!(routes.len(), 1);

        // Adding the same route again should not increase the list size
        routes = add_to_route_list(route, routes);
        assert_eq!(routes.len(), 1);
    }

    #[test]
    fn test_remove_from_route_list() {
        let route1 = RouteConfig {
            destination: IpNetwork::V4("10.0.1.0/24".parse().unwrap()),
            gateway: Some(IpAddr::V4("192.168.1.1".parse().unwrap())),
            interface_index: Some(1),
            metric: Some(100),
        };

        let route2 = RouteConfig {
            destination: IpNetwork::V4("10.0.2.0/24".parse().unwrap()),
            gateway: Some(IpAddr::V4("192.168.1.1".parse().unwrap())),
            interface_index: Some(1),
            metric: Some(100),
        };

        let routes = vec![route1.clone(), route2];
        let routes = remove_from_route_list(&route1, &routes);
        assert_eq!(routes.len(), 1);
    }

    #[test]
    fn test_route_manager_lease_routes() {
        let manager = RouteManager::new(1, "hostgw".to_string());
        
        let lease = NetworkLease {
            enable_ipv4: true,
            enable_ipv6: true,
            subnet: "10.0.1.0/24".parse().unwrap(),
            ipv6_subnet: Some("fc00::/64".parse().unwrap()),
            public_ip: "192.168.1.1".parse().unwrap(),
            public_ipv6: Some("fc00::1".parse().unwrap()),
        };

        let ipv4_route = manager.get_route_for_lease(&lease);
        assert!(ipv4_route.is_some());
        
        let route = ipv4_route.unwrap();
        assert_eq!(route.destination, IpNetwork::V4("10.0.1.0/24".parse().unwrap()));
        assert_eq!(route.gateway, Some(IpAddr::V4("192.168.1.1".parse().unwrap())));

        let ipv6_route = manager.get_v6_route_for_lease(&lease);
        assert!(ipv6_route.is_some());
        
        let route = ipv6_route.unwrap();
        assert_eq!(route.destination, IpNetwork::V6("fc00::/64".parse().unwrap()));
        assert_eq!(route.gateway, Some(IpAddr::V6("fc00::1".parse().unwrap())));
    }
}
