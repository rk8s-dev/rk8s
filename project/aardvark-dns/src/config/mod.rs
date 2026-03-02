use crate::backend::DNSBackend;
use crate::error::{AardvarkError, AardvarkResult};
use log::error;
use std::collections::HashMap;
use std::fs::{metadata, read_dir, read_to_string};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::vec::Vec;
pub mod constants;

// Parse configuration files in the given directory.
// Configuration files are formatted as follows:
// The name of the file will be interpreted as the name of the network.
// The first line must be the gateway IP(s) of the network, comma-separated.
// All subsequent individual lines contain info on a single container and are
// formatted as:
// <container ID, space, IPv4 address, space, IPv6 address, space, comma-separated list of name and aliases>
// Where space is a single space character.
// Returns a complete DNSBackend struct (all that is necessary for looks) and

// Silent clippy: sometimes clippy marks useful tyes as complex and for this case following type is
// convinient
#[allow(clippy::type_complexity)]
pub fn parse_configs(
    dir: &str,
    filter_search_domain: &str,
) -> AardvarkResult<(
    DNSBackend,
    HashMap<String, Vec<Ipv4Addr>>,
    HashMap<String, Vec<Ipv6Addr>>,
)> {
    if !metadata(dir)?.is_dir() {
        return Err(AardvarkError::msg(format!(
            "config directory {dir} must exist and be a directory"
        )));
    }

    let mut network_membership: HashMap<String, Vec<String>> = HashMap::new();
    let mut container_ips: HashMap<String, Vec<IpAddr>> = HashMap::new();
    let mut reverse: HashMap<String, HashMap<IpAddr, Vec<String>>> = HashMap::new();
    let mut network_names: HashMap<String, HashMap<String, Vec<IpAddr>>> = HashMap::new();
    let mut listen_ips_4: HashMap<String, Vec<Ipv4Addr>> = HashMap::new();
    let mut listen_ips_6: HashMap<String, Vec<Ipv6Addr>> = HashMap::new();
    let mut ctr_dns_server: HashMap<IpAddr, Option<Vec<IpAddr>>> = HashMap::new();
    let mut network_dns_server: HashMap<String, Vec<IpAddr>> = HashMap::new();
    let mut network_is_internal: HashMap<String, bool> = HashMap::new();

    // Enumerate all files in the directory, read them in one by one.
    // Steadily build a map of what container has what IPs and what
    // container is in what networks.
    let configs = read_dir(dir)?;
    for config in configs {
        // Each entry is a result. Interpret Err to mean the config was removed
        // while we were working; warn only, don't error.
        // Might be safer to completely restart the process, but there's also a
        // chance that, if we do that, we never finish and update the config,
        // assuming the files in question are modified at a sufficiently high
        // rate.
        match config {
            Ok(cfg) => {
                // dont process aardvark pid files
                if let Some(path) = cfg.path().file_name() {
                    if path == constants::AARDVARK_PID_FILE {
                        continue;
                    }
                }
                let parsed_network_config = match parse_config(cfg.path().as_path()) {
                    Ok(c) => c,
                    Err(e) => {
                        match &e {
                            AardvarkError::IOError(io)
                                if io.kind() != std::io::ErrorKind::NotFound =>
                            {
                                // Do no log the error if the file was removed
                            }
                            _ => {
                                error!(
                                    "Error reading config file {:?} for server update: {}",
                                    cfg.path(),
                                    e
                                )
                            }
                        }
                        continue;
                    }
                };

                let mut internal = false;

                let network_name: String = match cfg.path().file_name() {
                    // This isn't *completely* safe, but I do not foresee many
                    // cases where our network names include non-UTF8
                    // characters.
                    Some(s) => match s.to_str() {
                        Some(st) => {
			    let name_full = st.to_string();
			    if name_full.ends_with(constants::INTERNAL_SUFFIX) {
				internal = true;
			    }
			    name_full.strip_suffix(constants::INTERNAL_SUFFIX).unwrap_or(&name_full).to_string()
			},
                        None => return Err(AardvarkError::msg(
                            format!("configuration file {} name has non-UTF8 characters", s.to_string_lossy()),
                        )),
                    },
                    None => return Err(AardvarkError::msg(
                        format!("configuration file {} does not have a file name, cannot identify network name", cfg.path().to_string_lossy()),
                        )),
                };

                // Network DNS Servers were found while parsing config
                // lets populate the backend
                // Only if network is not internal.
                // If internal, explicitly insert empty list.
                if !parsed_network_config.network_dnsservers.is_empty() && !internal {
                    network_dns_server.insert(
                        network_name.clone(),
                        parsed_network_config.network_dnsservers,
                    );
                }
                if internal {
                    network_dns_server.insert(network_name.clone(), Vec::new());
                }

                for ip in parsed_network_config.network_bind_ip {
                    match ip {
                        IpAddr::V4(a) => listen_ips_4
                            .entry(network_name.clone())
                            .or_default()
                            .push(a),
                        IpAddr::V6(b) => listen_ips_6
                            .entry(network_name.clone())
                            .or_default()
                            .push(b),
                    }
                }

                for entry in parsed_network_config.container_entry {
                    // Container network membership
                    let ctr_networks = network_membership.entry(entry.id.clone()).or_default();

                    // Keep the network deduplicated
                    if !ctr_networks.contains(&network_name) {
                        ctr_networks.push(network_name.clone());
                    }

                    // Container IP addresses
                    let mut new_ctr_ips: Vec<IpAddr> = Vec::new();
                    if let Some(v4) = entry.v4 {
                        for ip in v4 {
                            reverse
                                .entry(network_name.clone())
                                .or_default()
                                .entry(IpAddr::V4(ip))
                                .or_default()
                                .append(&mut entry.aliases.clone());
                            // DNS only accepted on non-internal networks.
                            if !internal {
                                ctr_dns_server.insert(IpAddr::V4(ip), entry.dns_servers.clone());
                            }
                            new_ctr_ips.push(IpAddr::V4(ip));
                        }
                    }
                    if let Some(v6) = entry.v6 {
                        for ip in v6 {
                            reverse
                                .entry(network_name.clone())
                                .or_default()
                                .entry(IpAddr::V6(ip))
                                .or_default()
                                .append(&mut entry.aliases.clone());
                            // DNS only accepted on non-internal networks.
                            if !internal {
                                ctr_dns_server.insert(IpAddr::V6(ip), entry.dns_servers.clone());
                            }
                            new_ctr_ips.push(IpAddr::V6(ip));
                        }
                    }

                    let ctr_ips = container_ips.entry(entry.id.clone()).or_default();
                    ctr_ips.append(&mut new_ctr_ips.clone());

                    // Network aliases to IPs map.
                    let network_aliases = network_names.entry(network_name.clone()).or_default();
                    for alias in entry.aliases {
                        let alias_entries = network_aliases.entry(alias).or_default();
                        alias_entries.append(&mut new_ctr_ips.clone());
                    }

                    network_is_internal.insert(network_name.clone(), internal);
                }
            }
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    error!("Error listing config file for server update: {e}")
                }
            }
        }
    }

    // Set up types to be returned.
    let mut ctrs: HashMap<IpAddr, Vec<String>> = HashMap::new();

    for (ctr_id, ips) in container_ips {
        match network_membership.get(&ctr_id) {
            Some(s) => {
                for ip in ips {
                    let ip_networks = ctrs.entry(ip).or_default();
                    ip_networks.append(&mut s.clone());
                }
            }
            None => {
                return Err(AardvarkError::msg(format!(
                "Container ID {ctr_id} has an entry in IPs table, but not network membership table"
            )))
            }
        }
    }

    Ok((
        DNSBackend::new(
            ctrs,
            network_names,
            reverse,
            ctr_dns_server,
            network_dns_server,
            network_is_internal,
            filter_search_domain.to_owned(),
        ),
        listen_ips_4,
        listen_ips_6,
    ))
}

// A single entry in a config file
struct CtrEntry {
    id: String,
    v4: Option<Vec<Ipv4Addr>>,
    v6: Option<Vec<Ipv6Addr>>,
    aliases: Vec<String>,
    dns_servers: Option<Vec<IpAddr>>,
}

// A simplified type for results retured by
// parse_config after parsing a single network
// config.
struct ParsedNetworkConfig {
    network_bind_ip: Vec<IpAddr>,
    container_entry: Vec<CtrEntry>,
    network_dnsservers: Vec<IpAddr>,
}

// Read and parse a single given configuration file
fn parse_config(path: &std::path::Path) -> AardvarkResult<ParsedNetworkConfig> {
    let content = read_to_string(path)?;
    let mut is_first = true;

    let mut bind_addrs: Vec<IpAddr> = Vec::new();
    let mut network_dns_servers: Vec<IpAddr> = Vec::new();
    let mut ctrs: Vec<CtrEntry> = Vec::new();

    // Split on newline, parse each line
    for line in content.split('\n') {
        if line.is_empty() {
            continue;
        }
        if is_first {
            let network_parts = line.split(' ').collect::<Vec<&str>>();
            if network_parts.is_empty() {
                return Err(AardvarkError::msg(format!(
                    "invalid network configuration file: {}",
                    path.display()
                )));
            }
            // process bind ip
            for ip in network_parts[0].split(',') {
                let local_ip = match ip.parse() {
                    Ok(l) => l,
                    Err(e) => {
                        return Err(AardvarkError::msg(format!(
                            "error parsing ip address {ip}: {e}"
                        )))
                    }
                };
                bind_addrs.push(local_ip);
            }

            // If network parts contain more than one col then
            // we have custom dns server also defined at network level
            // lets process that.
            if network_parts.len() > 1 {
                for ip in network_parts[1].split(',') {
                    let local_ip = match ip.parse() {
                        Ok(l) => l,
                        Err(e) => {
                            return Err(AardvarkError::msg(format!(
                                "error parsing network dns address {ip}: {e}"
                            )))
                        }
                    };
                    network_dns_servers.push(local_ip);
                }
            }

            is_first = false;
            continue;
        }

        // Split on space
        let parts = line.split(' ').collect::<Vec<&str>>();
        if parts.len() < 4 {
            return Err(AardvarkError::msg(format!(
                "configuration file {} line {} is improperly formatted - too few entries",
                path.to_string_lossy(),
                line
            )));
        }

        let v4_addrs: Option<Vec<Ipv4Addr>> = if !parts[1].is_empty() {
            let ipv4 = match parts[1].split(',').map(|i| i.parse()).collect() {
                Ok(i) => i,
                Err(e) => {
                    return Err(AardvarkError::msg(format!(
                        "error parsing IP address {}: {}",
                        parts[1], e
                    )))
                }
            };
            Some(ipv4)
        } else {
            None
        };

        let v6_addrs: Option<Vec<Ipv6Addr>> = if !parts[2].is_empty() {
            let ipv6 = match parts[2].split(',').map(|i| i.parse()).collect() {
                Ok(i) => i,
                Err(e) => {
                    return Err(AardvarkError::msg(format!(
                        "error parsing IP address {}: {}",
                        parts[2], e
                    )))
                }
            };
            Some(ipv6)
        } else {
            None
        };

        let aliases: Vec<String> = parts[3]
            .split(',')
            .map(|x| x.to_string().to_lowercase())
            .collect::<Vec<String>>();

        if aliases.is_empty() {
            return Err(AardvarkError::msg(format!(
                "configuration file {} line {} is improperly formatted - no names given",
                path.to_string_lossy(),
                line
            )));
        }

        let dns_servers: Option<Vec<IpAddr>> = if parts.len() == 5 && !parts[4].is_empty() {
            let dns_server = match parts[4].split(',').map(|i| i.parse()).collect() {
                Ok(i) => i,
                Err(e) => {
                    return Err(AardvarkError::msg(format!(
                        "error parsing DNS server address {}: {}",
                        parts[4], e
                    )))
                }
            };
            Some(dns_server)
        } else {
            None
        };

        ctrs.push(CtrEntry {
            id: parts[0].to_string().to_lowercase(),
            v4: v4_addrs,
            v6: v6_addrs,
            aliases,
            dns_servers,
        });
    }

    // Must provide at least one bind address
    if bind_addrs.is_empty() {
        return Err(AardvarkError::msg(format!(
            "configuration file {} does not provide any bind addresses",
            path.to_string_lossy()
        )));
    }

    Ok(ParsedNetworkConfig {
        network_bind_ip: bind_addrs,
        container_entry: ctrs,
        network_dnsservers: network_dns_servers,
    })
}
