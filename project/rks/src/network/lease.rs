use std::net::{Ipv4Addr, Ipv6Addr};
use ipnetwork::{Ipv4Network, Ipv6Network};
use serde::{Serialize, Deserialize};
use serde_json::Value as JsonValue;
use chrono::{DateTime, Utc};
use log::error;

use crate::network::manager::Cursor;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[repr(u8)]
pub enum EventType {
    Added = 0,
    Removed = 1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    #[serde(rename = "type")]
    pub event_type: EventType,
    pub lease: Option<Lease>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaseAttrs {
    #[serde(rename = "PublicIP")]
    pub public_ip: Ipv4Addr,

    #[serde(rename = "PublicIPv6", skip_serializing_if = "Option::is_none")]
    pub public_ipv6: Option<Ipv6Addr>,

    #[serde(rename = "BackendType", skip_serializing_if = "String::is_empty")]
    pub backend_type: String,

    #[serde(rename = "BackendData", skip_serializing_if = "Option::is_none")]
    pub backend_data: Option<JsonValue>,

    #[serde(rename = "BackendV6Data", skip_serializing_if = "Option::is_none")]
    pub backend_v6_data: Option<JsonValue>,
}

impl Default for LeaseAttrs {
    fn default() -> Self {
        LeaseAttrs {
            public_ip: Ipv4Addr::UNSPECIFIED,
            public_ipv6: None,
            backend_type: String::new(),
            backend_data: None,
            backend_v6_data: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lease {
    pub enable_ipv4: bool,
    pub enable_ipv6: bool,
    pub subnet: Ipv4Network,
    pub ipv6_subnet: Ipv6Network,
    pub attrs: LeaseAttrs,
    pub expiration: DateTime<Utc>, // from chrono crate
    pub asof: Option<i64>, // only used in etcd
}

impl Default for Lease {
    fn default() -> Self {
        Lease {
            enable_ipv4: false,
            enable_ipv6: false,
            subnet: "0.0.0.0/32".parse().unwrap(),
            ipv6_subnet: "::/128".parse().unwrap(),
            attrs: LeaseAttrs::default(),
            expiration: Utc::now(),
            asof: None,
        }
    }
}


#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LeaseWatchResult {
    pub events: Vec<Event>,
    pub snapshot: Vec<Lease>, // only used in etcd
    pub cursor: Cursor, // Only used in etcd
}

#[derive(Debug, Clone)]
pub struct LeaseWatcher {
    pub own_lease: Lease,  // Lease with subnet of the local node
    pub leases: Vec<Lease> // Leases from other nodes
}

impl std::fmt::Display for LeaseAttrs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BackendType: {}, PublicIP: {}", self.backend_type, self.public_ip)?;
        write!(
            f,
            ", PublicIPv6: {}",
            self.public_ipv6.map_or("(nil)".to_string(), |v| v.to_string())
        )?;
        write!(
            f,
            ", BackendData: {}",
            self.backend_data
                .as_ref()
                .map(|d| d.to_string())
                .unwrap_or_else(|| "(nil)".to_string())
        )?;
        write!(
            f,
            ", BackendV6Data: {}",
            self.backend_v6_data
                .as_ref()
                .map(|d| d.to_string())
                .unwrap_or_else(|| "(nil)".to_string())
        )
    }
}

fn same_subnet(ipv4_enabled: bool, ipv6_enabled: bool, a: &Lease, b: &Lease) -> bool {
    match (ipv4_enabled, ipv6_enabled) {
        (true, false) => a.subnet == b.subnet,
        (false, true) => a.ipv6_subnet == b.ipv6_subnet,
        (true, true) => a.subnet == b.subnet && a.ipv6_subnet == b.ipv6_subnet,
        (false, false) => a.subnet == b.subnet, // etcd case fallback
    }
}

impl LeaseWatcher {
    pub fn reset(&mut self, new_leases: Vec<Lease>) -> Vec<Event> {
        let mut batch = Vec::new();

        for new_lease in &new_leases {
            if same_subnet(new_lease.enable_ipv4, new_lease.enable_ipv6, &self.own_lease, new_lease) {
                continue;
            }

            let mut found = false;
            self.leases.retain(|old_lease| {
                if same_subnet(old_lease.enable_ipv4, old_lease.enable_ipv6, old_lease, new_lease) {
                    found = true;
                    false // remove this old lease
                } else {
                    true // keep it
                }
            });

            if !found {
                batch.push(Event {
                    event_type: EventType::Added,
                    lease: Some(new_lease.clone()),
                });
            }
        }

        for removed in &self.leases {
            batch.push(Event {
                event_type: EventType::Removed,
                lease: Some(removed.clone()),
            });
        }

        // Update internal state
        self.leases = new_leases;

        batch
    }

    pub fn update(&mut self, events: Vec<Event>) -> Vec<Event> {
        let mut batch = Vec::new();

        for event in events {
            let lease = match &event.lease {
                Some(lease) => lease,
                None => continue,
            };

            if same_subnet(lease.enable_ipv4, lease.enable_ipv6, &self.own_lease, lease) {
                continue;
            }

            match event.event_type {
                EventType::Added => batch.push(self.add(lease)),
                EventType::Removed => batch.push(self.remove(lease)),
            }
        }

        batch
    }

    fn add(&mut self, lease: &Lease) -> Event {
        for existing in &mut self.leases {
            if same_subnet(existing.enable_ipv4, existing.enable_ipv6, existing, lease) {
                *existing = lease.clone();
                return Event {
                    event_type: EventType::Added,
                    lease: Some(existing.clone()),
                };
            }
        }

        self.leases.push(lease.clone());
        Event {
            event_type: EventType::Added,
            lease: Some(lease.clone()),
        }
    }

    fn remove(&mut self, lease: &Lease) -> Event {
        for i in 0..self.leases.len() {
            if same_subnet(self.leases[i].enable_ipv4, self.leases[i].enable_ipv6, &self.leases[i], lease) {
                let removed = self.leases.remove(i);
                return Event {
                    event_type: EventType::Removed,
                    lease: Some(removed),
                };
            }
        }

        // Not found, but still return an event
        error!(
            "Removed subnet ({}) and ipv6 subnet ({}) were not found",
            lease.subnet,
            lease.ipv6_subnet
        );
        Event {
            event_type: EventType::Removed,
            lease: Some(lease.clone()),
        }
    }
}
