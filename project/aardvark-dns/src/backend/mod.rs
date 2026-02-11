use log::error;
use std::collections::HashMap;
use std::net::IpAddr;
use std::vec::Vec;

// The core structure of the in-memory backing store for the DNS server.
// TODO: I've initially intermingled v4 and v6 addresses for simplicity; the
// server will get back a mix of responses and filter for v4/v6 from there.
// This may not be a good decision, not sure yet; we can split later if
// necessary.
pub struct DNSBackend {
    // Map of IP -> Network membership.
    // Every container must have an entry in this map, otherwise we will not
    // service requests to the Podman TLD for it.
    pub ip_mappings: HashMap<IpAddr, Vec<String>>,
    // Map of network name to map of name to IP addresses.
    pub name_mappings: HashMap<String, HashMap<String, Vec<IpAddr>>>,
    // Map of network name to map of IP address to container name.
    pub reverse_mappings: HashMap<String, HashMap<IpAddr, Vec<String>>>,
    // Map of IP address to DNS server IPs to service queries not handled
    // directly.
    pub ctr_dns_server: HashMap<IpAddr, Option<Vec<IpAddr>>>,
    // Map of network name and DNS server IPs.
    pub network_dns_server: HashMap<String, Vec<IpAddr>>,
    // Map of network name to bool (network is/is not internal)
    pub network_is_internal: HashMap<String, bool>,

    // search_domain used by aardvark-dns
    pub search_domain: String,
}

impl DNSBackend {
    // Create a new backend from the given set of network mappings.
    pub fn new(
        containers: HashMap<IpAddr, Vec<String>>,
        networks: HashMap<String, HashMap<String, Vec<IpAddr>>>,
        reverse: HashMap<String, HashMap<IpAddr, Vec<String>>>,
        ctr_dns_server: HashMap<IpAddr, Option<Vec<IpAddr>>>,
        network_dns_server: HashMap<String, Vec<IpAddr>>,
        network_is_internal: HashMap<String, bool>,
        mut search_domain: String,
    ) -> DNSBackend {
        // dns request always end with dot so append one for easier compare later
        if let Some(c) = search_domain.chars().rev().nth(0) {
            if c != '.' {
                search_domain.push('.')
            }
        }
        DNSBackend {
            ip_mappings: containers,
            name_mappings: networks,
            reverse_mappings: reverse,
            ctr_dns_server,
            network_dns_server,
            network_is_internal,
            search_domain,
        }
    }

    // Handle a single DNS lookup made by a given IP.
    // Returns all the ips for the given entry name
    pub fn lookup(
        &self,
        requester: &IpAddr,
        network_name: &str,
        entry: &str,
    ) -> Option<Vec<IpAddr>> {
        // Normalize lookup entry to lowercase.
        let mut name = entry.to_lowercase();

        // Trim off configured search domain if needed as keys do not contain it.
        // There doesn't seem to be a nicer way to do that:
        // https://users.rust-lang.org/t/can-strip-suffix-mutate-a-string-value/86852
        if name.ends_with(&self.search_domain) {
            name.truncate(name.len() - self.search_domain.len())
        }

        // if this is a fully qualified name, remove dots so backend can perform search
        if name.ends_with(".") {
            name.truncate(name.len() - 1)
        }

        let owned_netns: Vec<String>;

        let nets = match self.ip_mappings.get(requester) {
            Some(n) => n,
            // no source ip found let's just allow access to the current network where the request was made
            // On newer rust versions in CI we can return &vec![network_name.to_string()] directly without the extra assignment to the outer scope
            None => {
                owned_netns = vec![network_name.to_string()];
                &owned_netns
            }
        };

        let mut results: Vec<IpAddr> = Vec::new();

        for net in nets {
            let net_names = match self.name_mappings.get(net) {
                Some(n) => n,
                None => {
                    error!("Container with IP {requester} belongs to network {net} but there is no listing in networks table!");
                    continue;
                }
            };

            if let Some(addrs) = net_names.get(&name) {
                results.append(&mut addrs.clone());
            }
        }

        if results.is_empty() {
            return None;
        }

        Some(results)
    }

    // Returns list of network resolvers for a particular container
    pub fn get_network_scoped_resolvers(&self, requester: &IpAddr) -> Option<Vec<IpAddr>> {
        let mut results: Vec<IpAddr> = Vec::new();

        match self.ip_mappings.get(requester) {
            Some(nets) => {
                for net in nets {
                    match self.network_dns_server.get(net) {
                        Some(resolvers) => results.extend_from_slice(resolvers),
                        None => {
                            continue;
                        }
                    };
                }
            }
            None => return None,
        };

        Some(results)
    }

    // Checks if a container is associated with only internal networks.
    // Returns true if and only if a container is only present in
    // internal networks.
    pub fn ctr_is_internal(&self, requester: &IpAddr) -> bool {
        match self.ip_mappings.get(requester) {
            Some(nets) => {
                for net in nets {
                    match self.network_is_internal.get(net) {
                        Some(internal) => {
                            if !internal {
                                return false;
                            }
                        }
                        None => continue,
                    }
                }
            }
            // For safety, if we don't know about the IP, assume it's probably
            // someone on the host asking; let them access DNS.
            None => return false,
        }

        true
    }

    /// Return a single name resolved via mapping if it exists.
    pub fn reverse_lookup(&self, requester: &IpAddr, lookup_ip: &IpAddr) -> Option<&Vec<String>> {
        let nets = self.ip_mappings.get(requester)?;

        for ips in nets.iter().filter_map(|v| self.reverse_mappings.get(v)) {
            if let Some(names) = ips.get(lookup_ip) {
                return Some(names);
            }
        }

        None
    }
}
