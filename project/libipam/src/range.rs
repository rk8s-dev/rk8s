use std::net::IpAddr;
use crate::allocator::{last_ip, next_ip};

use cni_plugin::ip_range::IpRange;
use ipnetwork::IpNetwork;
use anyhow::bail;

pub trait IpRangeExt {
    fn canonicalize(&mut self) -> anyhow::Result<()>;
    fn contains(&self, addr: IpAddr) -> bool;
    fn overlap(&self, other: &Self) -> bool;
    fn format(&self) -> String; 
    fn default_range() -> Self;
}

impl IpRangeExt for IpRange {
    fn canonicalize(&mut self) -> anyhow::Result<()> {
        let prefix = self.subnet.prefix();
        if prefix >= 31 {
            bail!("Network {} too small to allocate from", self.subnet);
        }

        if self.subnet.ip() != self.subnet.network() {
            bail!(
                "Network has host bits set. For a subnet mask of length {} the network address is {}",
                self.subnet.prefix(),
                self.subnet.network()
            );
        }

        // Set default gateway if none
        if self.gateway.is_none() {
            self.gateway = next_ip(&self.subnet.ip());
        }

        // Validate or set range_start
        if let Some(range_start) = self.range_start {
            if !self.subnet.contains(range_start) {
                bail!("RangeStart {} not in network {}", range_start, self.subnet);
            }
        } else {
            self.range_start = next_ip(&self.subnet.ip());
        }

        // Validate or set range_end
        if let Some(range_end) = self.range_end {
            if !self.subnet.contains(range_end) {
                bail!("RangeEnd {} not in network {}", range_end, self.subnet);
            }
        } else {
            self.range_end = Some(last_ip(&self.subnet));
        }

        Ok(())
    }

    fn contains(&self, addr: IpAddr) -> bool {
        if !self.subnet.contains(addr) {
            return false;
        }
        if let Some(range_start) = self.range_start {
            if addr < range_start {
                return false;
            }
        }
        if let Some(range_end) = self.range_end {
            if addr > range_end {
                return false;
            }
        }
        true
    }

    fn overlap(&self, other: &Self) -> bool {
        // Different address families cannot overlap
        if self.subnet.is_ipv4() != other.subnet.is_ipv4() {
            return false;
        }

        // If either range is not fully specified, assume no overlap
        let (Some(self_start), Some(self_end)) = (self.range_start, self.range_end) else {
            return false;
        };
        let (Some(other_start), Some(other_end)) = (other.range_start, other.range_end) else {
            return false;
        };

        // Check if ranges overlap
        self_start <= other_end && other_start <= self_end
    }

    fn format(&self) -> String {
        let start_str = self.range_start.as_ref()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "<nil>".to_string());
    
        let end_str = self.range_end.as_ref()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "<nil>".to_string());
    
        format!("{}-{}", start_str, end_str)
    }

    fn default_range() -> Self {
        IpRange {
            range_start: None,
            range_end: None,
            subnet: IpNetwork::new(IpAddr::V4(std::net::Ipv4Addr::new(0, 0, 0, 0)), 0).unwrap(),
            gateway: None,
        }
    }
}
