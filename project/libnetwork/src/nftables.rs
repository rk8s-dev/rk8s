use anyhow::Result;
use common;
use nftables::{expr, schema, stmt, types};
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
};

// Mark used to tag service traffic for postrouting masquerade
const SERVICE_TRAFFIC_MARK: u32 = 0x4000;

const TABLE_NAME: &str = "rk8s";
const CHAIN_SERVICES: &str = "services";
const CHAIN_MASQUERADE: &str = "masquerade";
const MAP_CLUSTER_IPS: &str = "cluster_ips";
const MAP_NODE_PORTS: &str = "node_ports";
const ENV_LEGACY_ENDPOINT_CHAINS: &str = "RK8S_NFTABLES_LEGACY_ENDPOINT_CHAINS";

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
struct BackendElement {
    ip: String,
    port: u32,
    protocol: String,
}

struct ServicePortBackendElements {
    desired: Vec<BackendElement>,
    actual: Vec<BackendElement>,
}

impl ServicePortBackendElements {
    fn stale_actual_elements(&self) -> HashSet<BackendElement> {
        let desired: HashSet<BackendElement> = self.desired.iter().cloned().collect();
        self.actual
            .iter()
            .filter(|elem| !desired.contains(*elem))
            .cloned()
            .collect()
    }
}

fn use_legacy_endpoint_chains() -> bool {
    std::env::var(ENV_LEGACY_ENDPOINT_CHAINS)
        .map(|v| {
            let normalized = v.trim().to_ascii_lowercase();
            normalized == "1" || normalized == "true" || normalized == "on" || normalized == "yes"
        })
        .unwrap_or(false)
}

/// Generates raw nftables JSON used to initialize shared verdict maps.
/// This bypasses schema serialization limitations in nftables-rs for map type `verdict`.
pub fn generate_verdict_maps_init_raw_json() -> Result<String> {
    let payload = serde_json::json!({
        "nftables": [
            {
                "metainfo": {
                    "json_schema_version": 1
                }
            },
            {
                "table": {
                    "family": "ip",
                    "name": TABLE_NAME
                }
            },
            {
                "map": {
                    "family": "ip",
                    "table": TABLE_NAME,
                    "name": MAP_CLUSTER_IPS,
                    "type": ["inet_proto", "ipv4_addr", "inet_service"],
                    "map": "verdict"
                }
            },
            {
                "map": {
                    "family": "ip",
                    "table": TABLE_NAME,
                    "name": MAP_NODE_PORTS,
                    "type": ["inet_proto", "inet_service"],
                    "map": "verdict"
                }
            }
        ]
    });

    serde_json::to_string(&payload).map_err(|e| anyhow::anyhow!(e))
}

/// Generates the FULL configuration (Table, Base Chains, and all Service Chains).
/// Used for initialization.
pub fn generate_nftables_config(
    services: &[common::ServiceTask],
    endpoints: &[common::Endpoint],
) -> Result<String> {
    let mut objects: Vec<schema::NfObject> = Vec::new();

    // 1. Base Table
    objects.push(schema::NfObject::ListObject(schema::NfListObject::Table(
        schema::Table {
            family: types::NfFamily::IP,
            name: Cow::Borrowed(TABLE_NAME),
            ..Default::default()
        },
    )));

    // 2. Flush Table (Command)
    objects.push(schema::NfObject::CmdObject(schema::NfCmd::Flush(
        schema::FlushObject::Table(schema::Table {
            family: types::NfFamily::IP,
            name: Cow::Borrowed(TABLE_NAME),
            ..Default::default()
        }),
    )));

    // 3. Base Chains (NAT & Filter)
    let base_chains = vec![
        (
            "nat-prerouting",
            types::NfChainType::NAT,
            types::NfHook::Prerouting,
            -100,
            None,
        ),
        (
            "nat-output",
            types::NfChainType::NAT,
            types::NfHook::Output,
            -100,
            None,
        ),
        (
            "nat-postrouting",
            types::NfChainType::NAT,
            types::NfHook::Postrouting,
            100,
            None,
        ),
        (
            "filter-prerouting",
            types::NfChainType::Filter,
            types::NfHook::Prerouting,
            -110,
            Some(types::NfChainPolicy::Accept),
        ),
        (
            "filter-input",
            types::NfChainType::Filter,
            types::NfHook::Input,
            0,
            Some(types::NfChainPolicy::Accept),
        ),
        (
            "filter-forward",
            types::NfChainType::Filter,
            types::NfHook::Forward,
            0,
            Some(types::NfChainPolicy::Accept),
        ),
        (
            "filter-output",
            types::NfChainType::Filter,
            types::NfHook::Output,
            0,
            Some(types::NfChainPolicy::Accept),
        ),
    ];

    for (name, ctype, hook, prio, policy) in base_chains {
        objects.push(schema::NfObject::ListObject(schema::NfListObject::Chain(
            schema::Chain {
                family: types::NfFamily::IP,
                table: Cow::Borrowed(TABLE_NAME),
                name: Cow::Borrowed(name),
                _type: Some(ctype),
                hook: Some(hook),
                prio: Some(prio),
                policy,
                ..Default::default()
            },
        )));
    }

    // 4. Custom Chains
    let custom_chains = vec![CHAIN_SERVICES, CHAIN_MASQUERADE];
    for name in custom_chains {
        objects.push(schema::NfObject::ListObject(schema::NfListObject::Chain(
            schema::Chain {
                family: types::NfFamily::IP,
                table: Cow::Borrowed(TABLE_NAME),
                name: Cow::Borrowed(name),
                ..Default::default()
            },
        )));
    }

    // 5. Base Rules (Jumps)
    let jumps = vec![
        ("nat-prerouting", CHAIN_SERVICES),
        ("nat-output", CHAIN_SERVICES),
        ("nat-postrouting", CHAIN_MASQUERADE),
    ];
    for (chain, target) in jumps {
        objects.push(schema::NfObject::ListObject(schema::NfListObject::Rule(
            schema::Rule {
                family: types::NfFamily::IP,
                table: Cow::Borrowed(TABLE_NAME),
                chain: Cow::Borrowed(chain),
                expr: Cow::Owned(vec![stmt::Statement::Jump(stmt::JumpTarget {
                    target: Cow::Borrowed(target),
                })]),
                ..Default::default()
            },
        )));
    }

    // 6. Masquerade Rules (Granular policies)
    // Scenario 1: Pod → ClusterIP → Other Pod (identified by the service mark)
    // When traffic is marked, it indicates Pod-to-Pod traffic via Service, requiring SNAT
    objects.push(schema::NfObject::ListObject(schema::NfListObject::Rule(
        schema::Rule {
            family: types::NfFamily::IP,
            table: Cow::Borrowed(TABLE_NAME),
            chain: Cow::Borrowed(CHAIN_MASQUERADE),
            expr: Cow::Owned(vec![
                // Match service traffic marker
                stmt::Statement::Match(stmt::Match {
                    left: expr::Expression::Named(expr::NamedExpression::Meta(expr::Meta {
                        key: expr::MetaKey::Mark,
                    })),
                    op: stmt::Operator::EQ,
                    right: expr::Expression::Number(SERVICE_TRAFFIC_MARK),
                }),
                // Action: Masquerade
                stmt::Statement::Masquerade(None),
            ]),
            comment: Some(Cow::Borrowed("Pod → ClusterIP → Other Pod")),
            ..Default::default()
        },
    )));

    // Scenario 2: Pod → Same Node Pod - No handling (no rules, direct routing)
    // Scenario 3: Pod → Service → Self (Hairpin) - No handling (no rules, loopback)
    // Note: These two scenarios don't require rules; traffic passes naturally without masquerade

    // 7. Generate Service Chains (Full Sync)
    let mut parsed_endpoints = std::collections::HashMap::new();
    for ep in endpoints {
        parsed_endpoints.insert(
            (ep.metadata.namespace.clone(), ep.metadata.name.clone()),
            ep,
        );
    }

    for svc in services {
        let ep = parsed_endpoints
            .get(&(svc.metadata.namespace.clone(), svc.metadata.name.clone()))
            .map(|&e| e.clone())
            .unwrap_or_else(|| common::Endpoint {
                api_version: "v1".into(),
                kind: "Endpoints".into(),
                metadata: svc.metadata.clone(),
                subsets: vec![],
            });

        // Full Sync: Do not include delete commands for dispatch rules, as table is flushed.
        let update_objects = generate_service_update_objects(svc, &ep)?;
        objects.extend(update_objects);
    }

    // 8. Shared service-discovery (`services` chain + verdict map elements)
    // is initialized separately via `generate_services_discovery_refresh` and
    // then maintained incrementally via map element delta updates.

    let nftables = schema::Nftables {
        objects: Cow::Owned(objects),
    };
    serde_json::to_string(&nftables).map_err(|e| anyhow::anyhow!(e))
}

fn generate_service_update_objects(
    svc: &common::ServiceTask,
    ep: &common::Endpoint,
) -> Result<Vec<schema::NfObject<'static>>> {
    let mut objects = Vec::new();
    let legacy_endpoint_chains = use_legacy_endpoint_chains();

    for svc_port in &svc.spec.ports {
        let protocol = svc_port.protocol.to_lowercase();
        let chain_name = service_chain_name(svc, svc_port.port, &protocol);
        let mark_chain_name = service_mark_chain_name(&chain_name);

        // 1. Create Chain
        objects.push(schema::NfObject::ListObject(schema::NfListObject::Chain(
            schema::Chain {
                family: types::NfFamily::IP,
                table: Cow::Borrowed(TABLE_NAME),
                name: Cow::Owned(chain_name.clone()),
                ..Default::default()
            },
        )));

        // 2. Flush Chain
        objects.push(schema::NfObject::CmdObject(schema::NfCmd::Flush(
            schema::FlushObject::Chain(schema::Chain {
                family: types::NfFamily::IP,
                table: Cow::Borrowed(TABLE_NAME),
                name: Cow::Owned(chain_name.clone()),
                ..Default::default()
            }),
        )));

        // Mark chain is used by both full-sync and incremental paths for ClusterIP traffic.
        if valid_cluster_ip(svc).is_some() {
            objects.push(schema::NfObject::ListObject(schema::NfListObject::Chain(
                schema::Chain {
                    family: types::NfFamily::IP,
                    table: Cow::Borrowed(TABLE_NAME),
                    name: Cow::Owned(mark_chain_name.clone()),
                    ..Default::default()
                },
            )));

            objects.push(schema::NfObject::CmdObject(schema::NfCmd::Flush(
                schema::FlushObject::Chain(schema::Chain {
                    family: types::NfFamily::IP,
                    table: Cow::Borrowed(TABLE_NAME),
                    name: Cow::Owned(mark_chain_name.clone()),
                    ..Default::default()
                }),
            )));

            objects.push(schema::NfObject::ListObject(schema::NfListObject::Rule(
                schema::Rule {
                    family: types::NfFamily::IP,
                    table: Cow::Borrowed(TABLE_NAME),
                    chain: Cow::Owned(mark_chain_name),
                    expr: Cow::Owned(vec![
                        stmt::Statement::Mangle(stmt::Mangle {
                            key: expr::Expression::Named(expr::NamedExpression::Meta(expr::Meta {
                                key: expr::MetaKey::Mark,
                            })),
                            value: expr::Expression::Number(SERVICE_TRAFFIC_MARK),
                        }),
                        stmt::Statement::Jump(stmt::JumpTarget {
                            target: Cow::Owned(chain_name.clone()),
                        }),
                    ]),
                    ..Default::default()
                },
            )));
        }

        // 4. Build Backends
        let backends = backend_elements_for_service_port(svc_port, ep);

        // 5. Generate Rules in svc chain
        if backends.is_empty() {
            // Reject
            objects.push(schema::NfObject::ListObject(schema::NfListObject::Rule(
                schema::Rule {
                    family: types::NfFamily::IP,
                    table: Cow::Borrowed(TABLE_NAME),
                    chain: Cow::Owned(chain_name.clone()),
                    expr: Cow::Owned(vec![
                        stmt::Statement::Reject(None), // Reject with default type (icmp port-unreachable)
                    ]),
                    comment: Some(Cow::Borrowed("Reject (no endpoints)")),
                    ..Default::default()
                },
            )));
        } else if legacy_endpoint_chains {
            append_legacy_endpoint_chain_rules(&mut objects, &chain_name, &protocol, &backends);
        } else {
            append_service_port_dnat_map_rule(&mut objects, &chain_name, &protocol, &backends);
        }
    }

    Ok(objects)
}

fn generate_service_backend_only_update_objects(
    svc: &common::ServiceTask,
    ep: &common::Endpoint,
) -> Vec<schema::NfObject<'static>> {
    let mut objects = Vec::new();

    for svc_port in &svc.spec.ports {
        let protocol = svc_port.protocol.to_lowercase();
        let chain_name = service_chain_name(svc, svc_port.port, &protocol);

        // Endpoint-only incremental path: service/mark chains are expected to
        // already exist; only refresh backend dispatch rule in service chain.
        objects.push(schema::NfObject::CmdObject(schema::NfCmd::Flush(
            schema::FlushObject::Chain(schema::Chain {
                family: types::NfFamily::IP,
                table: Cow::Borrowed(TABLE_NAME),
                name: Cow::Owned(chain_name.clone()),
                ..Default::default()
            }),
        )));

        let backends = backend_elements_for_service_port(svc_port, ep);
        if backends.is_empty() {
            objects.push(schema::NfObject::ListObject(schema::NfListObject::Rule(
                schema::Rule {
                    family: types::NfFamily::IP,
                    table: Cow::Borrowed(TABLE_NAME),
                    chain: Cow::Owned(chain_name),
                    expr: Cow::Owned(vec![stmt::Statement::Reject(None)]),
                    comment: Some(Cow::Borrowed("Reject (no endpoints)")),
                    ..Default::default()
                },
            )));
        } else {
            append_service_port_dnat_map_rule(&mut objects, &chain_name, &protocol, &backends);
        }
    }

    objects
}

pub fn generate_service_update(svc: &common::ServiceTask, ep: &common::Endpoint) -> Result<String> {
    let objects = generate_service_update_objects(svc, ep)?;
    let nftables = schema::Nftables {
        objects: Cow::Owned(objects),
    };
    serde_json::to_string(&nftables).map_err(|e| anyhow::anyhow!(e))
}

pub fn generate_service_update_with_old_endpoint(
    svc: &common::ServiceTask,
    old_ep: &common::Endpoint,
    new_ep: &common::Endpoint,
) -> Result<String> {
    let legacy_endpoint_chains = use_legacy_endpoint_chains();
    let mut objects = if legacy_endpoint_chains {
        generate_service_update_objects(svc, new_ep)?
    } else {
        generate_service_backend_only_update_objects(svc, new_ep)
    };

    if legacy_endpoint_chains {
        for svc_port in &svc.spec.ports {
            let protocol = svc_port.protocol.to_lowercase();
            let chain_name = service_chain_name(svc, svc_port.port, &protocol);
            let element_state = service_port_backend_elements_state(svc_port, old_ep, new_ep);
            let stale_actual = element_state.stale_actual_elements();

            let mut old_bindings: HashMap<BackendElement, Vec<String>> = HashMap::new();
            for (elem, ep_chain_name) in
                legacy_endpoint_chain_bindings_for_service_port(&chain_name, svc_port, old_ep)
            {
                old_bindings.entry(elem).or_default().push(ep_chain_name);
            }

            for elem in stale_actual {
                if let Some(chain_names) = old_bindings.get(&elem) {
                    for ep_chain_name in chain_names {
                        objects.push(schema::NfObject::CmdObject(schema::NfCmd::Flush(
                            schema::FlushObject::Chain(schema::Chain {
                                family: types::NfFamily::IP,
                                table: Cow::Borrowed(TABLE_NAME),
                                name: Cow::Owned(ep_chain_name.clone()),
                                ..Default::default()
                            }),
                        )));

                        objects.push(schema::NfObject::CmdObject(schema::NfCmd::Delete(
                            schema::NfListObject::Chain(schema::Chain {
                                family: types::NfFamily::IP,
                                table: Cow::Borrowed(TABLE_NAME),
                                name: Cow::Owned(ep_chain_name.clone()),
                                ..Default::default()
                            }),
                        )));
                    }
                }
            }
        }
    }

    let nftables = schema::Nftables {
        objects: Cow::Owned(objects),
    };
    serde_json::to_string(&nftables).map_err(|e| anyhow::anyhow!(e))
}

pub fn generate_service_detach_with_endpoint(
    svc: &common::ServiceTask,
    ep: Option<&common::Endpoint>,
) -> Result<String> {
    let mut objects = Vec::new();
    let legacy_endpoint_chains = use_legacy_endpoint_chains();

    for svc_port in &svc.spec.ports {
        let protocol = svc_port.protocol.to_lowercase();
        let chain_name = service_chain_name(svc, svc_port.port, &protocol);
        let mark_chain_name = service_mark_chain_name(&chain_name);

        // Phase 1: break references only (safe in one transaction with discovery refresh).
        if valid_cluster_ip(svc).is_some() {
            objects.push(schema::NfObject::CmdObject(schema::NfCmd::Flush(
                schema::FlushObject::Chain(schema::Chain {
                    family: types::NfFamily::IP,
                    table: Cow::Borrowed(TABLE_NAME),
                    name: Cow::Owned(mark_chain_name),
                    ..Default::default()
                }),
            )));
        }

        objects.push(schema::NfObject::CmdObject(schema::NfCmd::Flush(
            schema::FlushObject::Chain(schema::Chain {
                family: types::NfFamily::IP,
                table: Cow::Borrowed(TABLE_NAME),
                name: Cow::Owned(chain_name.clone()),
                ..Default::default()
            }),
        )));

        if legacy_endpoint_chains {
            let endpoint_chain_names =
                endpoint_chain_names_for_service_port(&chain_name, svc_port, ep);
            for ep_chain_name in endpoint_chain_names {
                objects.push(schema::NfObject::CmdObject(schema::NfCmd::Flush(
                    schema::FlushObject::Chain(schema::Chain {
                        family: types::NfFamily::IP,
                        table: Cow::Borrowed(TABLE_NAME),
                        name: Cow::Owned(ep_chain_name),
                        ..Default::default()
                    }),
                )));
            }
        }
    }

    let nftables = schema::Nftables {
        objects: Cow::Owned(objects),
    };
    serde_json::to_string(&nftables).map_err(|e| anyhow::anyhow!(e))
}

pub fn generate_service_delete_with_endpoint(
    svc: &common::ServiceTask,
    ep: Option<&common::Endpoint>,
) -> Result<String> {
    let mut objects = Vec::new();
    let legacy_endpoint_chains = use_legacy_endpoint_chains();

    for svc_port in &svc.spec.ports {
        let protocol = svc_port.protocol.to_lowercase();
        let chain_name = service_chain_name(svc, svc_port.port, &protocol);
        let mark_chain_name = service_mark_chain_name(&chain_name);

        // 1) Remove mark->svc reference first, then svc rule refs.
        if valid_cluster_ip(svc).is_some() {
            objects.push(schema::NfObject::CmdObject(schema::NfCmd::Flush(
                schema::FlushObject::Chain(schema::Chain {
                    family: types::NfFamily::IP,
                    table: Cow::Borrowed(TABLE_NAME),
                    name: Cow::Owned(mark_chain_name.clone()),
                    ..Default::default()
                }),
            )));
        }

        objects.push(schema::NfObject::CmdObject(schema::NfCmd::Flush(
            schema::FlushObject::Chain(schema::Chain {
                family: types::NfFamily::IP,
                table: Cow::Borrowed(TABLE_NAME),
                name: Cow::Owned(chain_name.clone()),
                ..Default::default()
            }),
        )));

        // 2) Remove endpoint chains that were referenced by svc chain.
        if legacy_endpoint_chains {
            let endpoint_chain_names =
                endpoint_chain_names_for_service_port(&chain_name, svc_port, ep);
            for ep_chain_name in endpoint_chain_names {
                objects.push(schema::NfObject::CmdObject(schema::NfCmd::Flush(
                    schema::FlushObject::Chain(schema::Chain {
                        family: types::NfFamily::IP,
                        table: Cow::Borrowed(TABLE_NAME),
                        name: Cow::Owned(ep_chain_name.clone()),
                        ..Default::default()
                    }),
                )));

                objects.push(schema::NfObject::CmdObject(schema::NfCmd::Delete(
                    schema::NfListObject::Chain(schema::Chain {
                        family: types::NfFamily::IP,
                        table: Cow::Borrowed(TABLE_NAME),
                        name: Cow::Owned(ep_chain_name),
                        ..Default::default()
                    }),
                )));
            }
        }

        // 3) Delete per-service chains.
        objects.push(schema::NfObject::CmdObject(schema::NfCmd::Delete(
            schema::NfListObject::Chain(schema::Chain {
                family: types::NfFamily::IP,
                table: Cow::Borrowed(TABLE_NAME),
                name: Cow::Owned(chain_name),
                ..Default::default()
            }),
        )));

        if valid_cluster_ip(svc).is_some() {
            objects.push(schema::NfObject::CmdObject(schema::NfCmd::Delete(
                schema::NfListObject::Chain(schema::Chain {
                    family: types::NfFamily::IP,
                    table: Cow::Borrowed(TABLE_NAME),
                    name: Cow::Owned(mark_chain_name),
                    ..Default::default()
                }),
            )));
        }
    }

    let nftables = schema::Nftables {
        objects: Cow::Owned(objects),
    };
    serde_json::to_string(&nftables).map_err(|e| anyhow::anyhow!(e))
}

pub fn generate_services_discovery_refresh(services: &[common::ServiceTask]) -> Result<String> {
    let mut objects = Vec::new();

    // Refresh shared services discovery chain in-place for incremental service changes.
    objects.push(schema::NfObject::CmdObject(schema::NfCmd::Flush(
        schema::FlushObject::Chain(schema::Chain {
            family: types::NfFamily::IP,
            table: Cow::Borrowed(TABLE_NAME),
            name: Cow::Borrowed(CHAIN_SERVICES),
            ..Default::default()
        }),
    )));

    // Use named verdict maps for discovery lookups.
    append_cluster_ip_named_map_lookup_rule(&mut objects, "tcp");
    append_cluster_ip_named_map_lookup_rule(&mut objects, "udp");
    append_nodeport_named_map_lookup_rule(&mut objects, "tcp");
    append_nodeport_named_map_lookup_rule(&mut objects, "udp");

    // Refresh map elements in-place to reflect current service snapshot.
    objects.push(schema::NfObject::CmdObject(schema::NfCmd::Flush(
        schema::FlushObject::Map(Box::new(schema::Map {
            family: types::NfFamily::IP,
            table: Cow::Borrowed(TABLE_NAME),
            name: Cow::Borrowed(MAP_CLUSTER_IPS),
            ..Default::default()
        })),
    )));

    objects.push(schema::NfObject::CmdObject(schema::NfCmd::Flush(
        schema::FlushObject::Map(Box::new(schema::Map {
            family: types::NfFamily::IP,
            table: Cow::Borrowed(TABLE_NAME),
            name: Cow::Borrowed(MAP_NODE_PORTS),
            ..Default::default()
        })),
    )));

    let lookup = build_service_lookup_maps(services);
    let cluster_entries = lookup
        .service_ips_tcp
        .into_iter()
        .chain(lookup.service_ips_udp)
        .filter_map(set_item_to_map_elem_expr)
        .collect::<Vec<_>>();
    let nodeport_entries = lookup
        .service_nodeports_tcp
        .into_iter()
        .chain(lookup.service_nodeports_udp)
        .filter_map(set_item_to_map_elem_expr)
        .collect::<Vec<_>>();

    if !cluster_entries.is_empty() {
        objects.push(schema::NfObject::CmdObject(schema::NfCmd::Add(
            schema::NfListObject::Element(schema::Element {
                family: types::NfFamily::IP,
                table: Cow::Borrowed(TABLE_NAME),
                name: Cow::Borrowed(MAP_CLUSTER_IPS),
                elem: Cow::Owned(cluster_entries),
            }),
        )));
    }

    if !nodeport_entries.is_empty() {
        objects.push(schema::NfObject::CmdObject(schema::NfCmd::Add(
            schema::NfListObject::Element(schema::Element {
                family: types::NfFamily::IP,
                table: Cow::Borrowed(TABLE_NAME),
                name: Cow::Borrowed(MAP_NODE_PORTS),
                elem: Cow::Owned(nodeport_entries),
            }),
        )));
    }

    let nftables = schema::Nftables {
        objects: Cow::Owned(objects),
    };
    serde_json::to_string(&nftables).map_err(|e| anyhow::anyhow!(e))
}

pub fn generate_services_discovery_delta(
    old_services: &[common::ServiceTask],
    new_services: &[common::ServiceTask],
) -> Result<String> {
    let old_lookup = build_service_lookup_maps(old_services);
    let new_lookup = build_service_lookup_maps(new_services);

    let (old_cluster, old_nodeports) = lookup_to_named_map_expr_maps(old_lookup);
    let (new_cluster, new_nodeports) = lookup_to_named_map_expr_maps(new_lookup);

    let mut objects: Vec<schema::NfObject<'static>> = Vec::new();

    // ClusterIP map delta.
    let cluster_deletes = old_cluster
        .iter()
        .filter(|(k, _)| !new_cluster.contains_key(*k))
        .map(|(_, v)| v.clone())
        .collect::<Vec<_>>();
    let cluster_adds = new_cluster
        .iter()
        .filter(|(k, _)| !old_cluster.contains_key(*k))
        .map(|(_, v)| v.clone())
        .collect::<Vec<_>>();

    if !cluster_deletes.is_empty() {
        objects.push(schema::NfObject::CmdObject(schema::NfCmd::Delete(
            schema::NfListObject::Element(schema::Element {
                family: types::NfFamily::IP,
                table: Cow::Borrowed(TABLE_NAME),
                name: Cow::Borrowed(MAP_CLUSTER_IPS),
                elem: Cow::Owned(cluster_deletes),
            }),
        )));
    }
    if !cluster_adds.is_empty() {
        objects.push(schema::NfObject::CmdObject(schema::NfCmd::Add(
            schema::NfListObject::Element(schema::Element {
                family: types::NfFamily::IP,
                table: Cow::Borrowed(TABLE_NAME),
                name: Cow::Borrowed(MAP_CLUSTER_IPS),
                elem: Cow::Owned(cluster_adds),
            }),
        )));
    }

    // NodePort map delta.
    let nodeport_deletes = old_nodeports
        .iter()
        .filter(|(k, _)| !new_nodeports.contains_key(*k))
        .map(|(_, v)| v.clone())
        .collect::<Vec<_>>();
    let nodeport_adds = new_nodeports
        .iter()
        .filter(|(k, _)| !old_nodeports.contains_key(*k))
        .map(|(_, v)| v.clone())
        .collect::<Vec<_>>();

    if !nodeport_deletes.is_empty() {
        objects.push(schema::NfObject::CmdObject(schema::NfCmd::Delete(
            schema::NfListObject::Element(schema::Element {
                family: types::NfFamily::IP,
                table: Cow::Borrowed(TABLE_NAME),
                name: Cow::Borrowed(MAP_NODE_PORTS),
                elem: Cow::Owned(nodeport_deletes),
            }),
        )));
    }
    if !nodeport_adds.is_empty() {
        objects.push(schema::NfObject::CmdObject(schema::NfCmd::Add(
            schema::NfListObject::Element(schema::Element {
                family: types::NfFamily::IP,
                table: Cow::Borrowed(TABLE_NAME),
                name: Cow::Borrowed(MAP_NODE_PORTS),
                elem: Cow::Owned(nodeport_adds),
            }),
        )));
    }

    let nftables = schema::Nftables {
        objects: Cow::Owned(objects),
    };
    serde_json::to_string(&nftables).map_err(|e| anyhow::anyhow!(e))
}

#[derive(Default)]
struct ServiceLookupMaps {
    service_ips_tcp: Vec<expr::SetItem<'static>>,
    service_ips_udp: Vec<expr::SetItem<'static>>,
    service_nodeports_tcp: Vec<expr::SetItem<'static>>,
    service_nodeports_udp: Vec<expr::SetItem<'static>>,
}

fn service_chain_name(svc: &common::ServiceTask, port: i32, protocol: &str) -> String {
    format!(
        "svc-{}-{}-{}-{}",
        svc.metadata.namespace, svc.metadata.name, protocol, port
    )
}

fn endpoint_chain_name(service_chain: &str, idx: usize) -> String {
    format!("ep-{}-{}", service_chain, idx)
}

fn service_mark_chain_name(service_chain: &str) -> String {
    format!("mark-{}", service_chain)
}

fn valid_cluster_ip(svc: &common::ServiceTask) -> Option<&str> {
    match svc.spec.cluster_ip.as_deref() {
        Some(ip) if !ip.is_empty() && !ip.eq_ignore_ascii_case("none") => Some(ip),
        _ => None,
    }
}

fn build_service_lookup_maps(services: &[common::ServiceTask]) -> ServiceLookupMaps {
    let mut maps = ServiceLookupMaps::default();
    let mut seen_service_ips = HashSet::new();
    let mut seen_nodeports = HashSet::new();

    for svc in services {
        let cluster_ip = valid_cluster_ip(svc);
        for svc_port in &svc.spec.ports {
            let protocol = svc_port.protocol.to_lowercase();
            let chain_name = service_chain_name(svc, svc_port.port, &protocol);
            let mark_chain_name = service_mark_chain_name(&chain_name);

            if let Some(cluster_ip) = cluster_ip {
                let dedup_key = format!("{}|{}|{}", cluster_ip, protocol, svc_port.port);
                if seen_service_ips.insert(dedup_key) {
                    let entry = expr::SetItem::MappingStatement(
                        service_ips_map_key_expr(cluster_ip, &protocol, svc_port.port as u32),
                        stmt::Statement::Jump(stmt::JumpTarget {
                            target: Cow::Owned(mark_chain_name),
                        }),
                    );

                    if protocol == "udp" {
                        maps.service_ips_udp.push(entry);
                    } else {
                        maps.service_ips_tcp.push(entry);
                    }
                }
            }

            if let Some(node_port) = svc_port.node_port {
                let dedup_key = format!("{}|{}", protocol, node_port);
                if seen_nodeports.insert(dedup_key) {
                    let entry = expr::SetItem::MappingStatement(
                        nodeports_map_key_expr(&protocol, node_port as u32),
                        stmt::Statement::Jump(stmt::JumpTarget {
                            target: Cow::Owned(chain_name),
                        }),
                    );

                    if protocol == "udp" {
                        maps.service_nodeports_udp.push(entry);
                    } else {
                        maps.service_nodeports_tcp.push(entry);
                    }
                }
            }
        }
    }

    maps
}

fn append_cluster_ip_named_map_lookup_rule(
    objects: &mut Vec<schema::NfObject<'static>>,
    protocol: &str,
) {
    objects.push(schema::NfObject::ListObject(schema::NfListObject::Rule(
        schema::Rule {
            family: types::NfFamily::IP,
            table: Cow::Borrowed(TABLE_NAME),
            chain: Cow::Borrowed(CHAIN_SERVICES),
            expr: Cow::Owned(vec![
                l4proto_match_statement(protocol.to_owned()),
                stmt::Statement::VerdictMap(stmt::VerdictMap {
                    key: cluster_ip_lookup_key_expr(protocol.to_owned()),
                    data: expr::Expression::String(Cow::Owned(format!("@{}", MAP_CLUSTER_IPS))),
                }),
            ]),
            ..Default::default()
        },
    )));
}

fn append_nodeport_named_map_lookup_rule(
    objects: &mut Vec<schema::NfObject<'static>>,
    protocol: &str,
) {
    objects.push(schema::NfObject::ListObject(schema::NfListObject::Rule(
        schema::Rule {
            family: types::NfFamily::IP,
            table: Cow::Borrowed(TABLE_NAME),
            chain: Cow::Borrowed(CHAIN_SERVICES),
            expr: Cow::Owned(vec![
                l4proto_match_statement(protocol.to_owned()),
                stmt::Statement::VerdictMap(stmt::VerdictMap {
                    key: nodeport_lookup_key_expr(protocol.to_owned()),
                    data: expr::Expression::String(Cow::Owned(format!("@{}", MAP_NODE_PORTS))),
                }),
            ]),
            ..Default::default()
        },
    )));
}

fn l4proto_match_statement(protocol: String) -> stmt::Statement<'static> {
    stmt::Statement::Match(stmt::Match {
        left: expr::Expression::Named(expr::NamedExpression::Meta(expr::Meta {
            key: expr::MetaKey::L4proto,
        })),
        op: stmt::Operator::EQ,
        right: expr::Expression::String(Cow::Owned(protocol)),
    })
}

fn cluster_ip_lookup_key_expr(protocol: String) -> expr::Expression<'static> {
    expr::Expression::Named(expr::NamedExpression::Concat(vec![
        expr::Expression::Named(expr::NamedExpression::Payload(expr::Payload::PayloadField(
            expr::PayloadField {
                protocol: Cow::Borrowed("ip"),
                field: Cow::Borrowed("protocol"),
            },
        ))),
        expr::Expression::Named(expr::NamedExpression::Payload(expr::Payload::PayloadField(
            expr::PayloadField {
                protocol: Cow::Borrowed("ip"),
                field: Cow::Borrowed("daddr"),
            },
        ))),
        expr::Expression::Named(expr::NamedExpression::Payload(expr::Payload::PayloadField(
            expr::PayloadField {
                protocol: Cow::Owned(protocol),
                field: Cow::Borrowed("dport"),
            },
        ))),
    ]))
}

fn nodeport_lookup_key_expr(protocol: String) -> expr::Expression<'static> {
    expr::Expression::Named(expr::NamedExpression::Concat(vec![
        expr::Expression::Named(expr::NamedExpression::Payload(expr::Payload::PayloadField(
            expr::PayloadField {
                protocol: Cow::Borrowed("ip"),
                field: Cow::Borrowed("protocol"),
            },
        ))),
        expr::Expression::Named(expr::NamedExpression::Payload(expr::Payload::PayloadField(
            expr::PayloadField {
                protocol: Cow::Owned(protocol),
                field: Cow::Borrowed("dport"),
            },
        ))),
    ]))
}

fn service_ips_map_key_expr(
    cluster_ip: &str,
    protocol: &str,
    port: u32,
) -> expr::Expression<'static> {
    expr::Expression::Named(expr::NamedExpression::Concat(vec![
        expr::Expression::Number(l4proto_number(protocol)),
        expr::Expression::String(Cow::Owned(cluster_ip.to_string())),
        expr::Expression::Number(port),
    ]))
}

fn l4proto_number(protocol: &str) -> u32 {
    match protocol {
        "udp" => 17,
        _ => 6,
    }
}

fn nodeports_map_key_expr(protocol: &str, port: u32) -> expr::Expression<'static> {
    expr::Expression::Named(expr::NamedExpression::Concat(vec![
        expr::Expression::Number(l4proto_number(protocol)),
        expr::Expression::Number(port),
    ]))
}

fn set_item_to_map_elem_expr(item: expr::SetItem<'static>) -> Option<expr::Expression<'static>> {
    match item {
        expr::SetItem::MappingStatement(key, stmt::Statement::Jump(target)) => {
            Some(expr::Expression::List(vec![
                key,
                expr::Expression::Verdict(expr::Verdict::Jump(target)),
            ]))
        }
        expr::SetItem::MappingStatement(key, stmt::Statement::Goto(target)) => {
            Some(expr::Expression::List(vec![
                key,
                expr::Expression::Verdict(expr::Verdict::Goto(target)),
            ]))
        }
        _ => None,
    }
}

fn map_expr_identity(expr: &expr::Expression<'static>) -> Option<String> {
    serde_json::to_string(expr).ok()
}

fn lookup_to_named_map_expr_maps(
    lookup: ServiceLookupMaps,
) -> (
    HashMap<String, expr::Expression<'static>>,
    HashMap<String, expr::Expression<'static>>,
) {
    fn collect(
        entries: Vec<expr::SetItem<'static>>,
        out: &mut HashMap<String, expr::Expression<'static>>,
    ) {
        for entry in entries {
            if let Some(expr) = set_item_to_map_elem_expr(entry)
                && let Some(key) = map_expr_identity(&expr)
            {
                out.insert(key, expr);
            }
        }
    }

    let mut cluster = HashMap::new();
    let mut nodeports = HashMap::new();

    collect(lookup.service_ips_tcp, &mut cluster);
    collect(lookup.service_ips_udp, &mut cluster);
    collect(lookup.service_nodeports_tcp, &mut nodeports);
    collect(lookup.service_nodeports_udp, &mut nodeports);

    (cluster, nodeports)
}

fn endpoint_chain_names_for_service_port(
    service_chain_name: &str,
    svc_port: &common::ServicePort,
    ep: Option<&common::Endpoint>,
) -> Vec<String> {
    let Some(ep) = ep else {
        return Vec::new();
    };

    let mut names = Vec::new();
    let mut idx = 0usize;

    for subset in &ep.subsets {
        let target_port = subset
            .ports
            .iter()
            .find(|p| match (&svc_port.name, &p.name) {
                (Some(n1), Some(n2)) => n1 == n2,
                (None, None) => true,
                (None, Some(_)) => false,
                _ => false,
            });

        if target_port.is_some() {
            for _ in &subset.addresses {
                names.push(endpoint_chain_name(service_chain_name, idx));
                idx += 1;
            }
        }
    }

    names
}

fn backend_elements_for_service_port(
    svc_port: &common::ServicePort,
    ep: &common::Endpoint,
) -> Vec<BackendElement> {
    let protocol = svc_port.protocol.to_lowercase();
    let mut elements = Vec::new();

    for subset in &ep.subsets {
        let target_port = subset
            .ports
            .iter()
            .find(|p| match (&svc_port.name, &p.name) {
                (Some(n1), Some(n2)) => n1 == n2,
                (None, None) => true,
                (None, Some(_)) => false,
                _ => false,
            });

        if let Some(tp) = target_port {
            for addr in &subset.addresses {
                elements.push(BackendElement {
                    ip: addr.ip.clone(),
                    port: tp.port as u32,
                    protocol: protocol.clone(),
                });
            }
        }
    }

    elements
}

fn service_port_backend_elements_state(
    svc_port: &common::ServicePort,
    old_ep: &common::Endpoint,
    new_ep: &common::Endpoint,
) -> ServicePortBackendElements {
    ServicePortBackendElements {
        desired: backend_elements_for_service_port(svc_port, new_ep),
        actual: backend_elements_for_service_port(svc_port, old_ep),
    }
}

fn append_service_port_dnat_map_rule(
    objects: &mut Vec<schema::NfObject<'static>>,
    chain_name: &str,
    protocol: &str,
    backends: &[BackendElement],
) {
    let num_backends = backends.len() as u32;
    let map_data = backends
        .iter()
        .enumerate()
        .map(|(idx, backend)| {
            expr::SetItem::Mapping(
                expr::Expression::Number(idx as u32),
                expr::Expression::Named(expr::NamedExpression::Concat(vec![
                    expr::Expression::String(Cow::Owned(backend.ip.clone())),
                    expr::Expression::Number(backend.port),
                ])),
            )
        })
        .collect::<Vec<_>>();

    objects.push(schema::NfObject::ListObject(schema::NfListObject::Rule(
        schema::Rule {
            family: types::NfFamily::IP,
            table: Cow::Borrowed(TABLE_NAME),
            chain: Cow::Owned(chain_name.to_string()),
            expr: Cow::Owned(vec![
                l4proto_match_statement(protocol.to_string()),
                stmt::Statement::DNAT(Some(stmt::NAT {
                    addr: Some(expr::Expression::Named(expr::NamedExpression::Map(
                        Box::new(expr::Map {
                            key: expr::Expression::Named(expr::NamedExpression::Numgen(
                                expr::Numgen {
                                    mode: expr::NgMode::Random,
                                    ng_mod: num_backends,
                                    offset: Some(0),
                                },
                            )),
                            data: expr::Expression::Named(expr::NamedExpression::Set(map_data)),
                        }),
                    ))),
                    family: Some(stmt::NATFamily::IP),
                    port: None,
                    flags: None,
                })),
            ]),
            comment: Some(Cow::Borrowed("LB: l4proto match + dnat numgen map")),
            ..Default::default()
        },
    )));
}

fn append_legacy_endpoint_chain_rules(
    objects: &mut Vec<schema::NfObject<'static>>,
    chain_name: &str,
    protocol: &str,
    backends: &[BackendElement],
) {
    let num_backends = backends.len() as u32;
    let mut lb_map = Vec::with_capacity(backends.len());

    for (i, backend) in backends.iter().enumerate() {
        let endpoint_chain = endpoint_chain_name(chain_name, i);

        objects.push(schema::NfObject::ListObject(schema::NfListObject::Chain(
            schema::Chain {
                family: types::NfFamily::IP,
                table: Cow::Borrowed(TABLE_NAME),
                name: Cow::Owned(endpoint_chain.clone()),
                ..Default::default()
            },
        )));

        objects.push(schema::NfObject::CmdObject(schema::NfCmd::Flush(
            schema::FlushObject::Chain(schema::Chain {
                family: types::NfFamily::IP,
                table: Cow::Borrowed(TABLE_NAME),
                name: Cow::Owned(endpoint_chain.clone()),
                ..Default::default()
            }),
        )));

        objects.push(schema::NfObject::ListObject(schema::NfListObject::Rule(
            schema::Rule {
                family: types::NfFamily::IP,
                table: Cow::Borrowed(TABLE_NAME),
                chain: Cow::Owned(endpoint_chain.clone()),
                expr: Cow::Owned(vec![
                    stmt::Statement::Match(stmt::Match {
                        left: expr::Expression::Named(expr::NamedExpression::Meta(expr::Meta {
                            key: expr::MetaKey::L4proto,
                        })),
                        op: stmt::Operator::EQ,
                        right: expr::Expression::String(Cow::Owned(protocol.to_string())),
                    }),
                    stmt::Statement::DNAT(Some(stmt::NAT {
                        addr: Some(expr::Expression::String(Cow::Owned(backend.ip.clone()))),
                        family: Some(stmt::NATFamily::IP),
                        port: Some(expr::Expression::Number(backend.port)),
                        flags: None,
                    })),
                ]),
                ..Default::default()
            },
        )));

        lb_map.push(expr::SetItem::MappingStatement(
            expr::Expression::Number(i as u32),
            stmt::Statement::Goto(stmt::JumpTarget {
                target: Cow::Owned(endpoint_chain),
            }),
        ));
    }

    objects.push(schema::NfObject::ListObject(schema::NfListObject::Rule(
        schema::Rule {
            family: types::NfFamily::IP,
            table: Cow::Borrowed(TABLE_NAME),
            chain: Cow::Owned(chain_name.to_string()),
            expr: Cow::Owned(vec![stmt::Statement::VerdictMap(stmt::VerdictMap {
                key: expr::Expression::Named(expr::NamedExpression::Numgen(expr::Numgen {
                    mode: expr::NgMode::Random,
                    ng_mod: num_backends,
                    offset: Some(0),
                })),
                data: expr::Expression::Named(expr::NamedExpression::Set(lb_map)),
            })]),
            comment: Some(Cow::Borrowed("LB: numgen random mod -> endpoint vmap")),
            ..Default::default()
        },
    )));
}

fn legacy_endpoint_chain_bindings_for_service_port(
    service_chain_name: &str,
    svc_port: &common::ServicePort,
    ep: &common::Endpoint,
) -> Vec<(BackendElement, String)> {
    backend_elements_for_service_port(svc_port, ep)
        .into_iter()
        .enumerate()
        .map(|(idx, elem)| (elem, endpoint_chain_name(service_chain_name, idx)))
        .collect()
}
