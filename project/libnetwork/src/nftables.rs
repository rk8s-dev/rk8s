use anyhow::Result;
use common;
use nftables::{expr, schema, stmt, types};
use serde_json::json;
use std::borrow::Cow;

// Mark used to tag service traffic for postrouting masquerade
const SERVICE_TRAFFIC_MARK: u32 = 0x4000;

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
            name: Cow::Borrowed("rk8s"),
            ..Default::default()
        },
    )));

    // 2. Flush Table (Command)
    objects.push(schema::NfObject::CmdObject(schema::NfCmd::Flush(
        schema::FlushObject::Table(schema::Table {
            family: types::NfFamily::IP,
            name: Cow::Borrowed("rk8s"),
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
                table: Cow::Borrowed("rk8s"),
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
    let custom_chains = vec!["services", "services_tcp", "services_udp", "masquerade"];
    for name in custom_chains {
        objects.push(schema::NfObject::ListObject(schema::NfListObject::Chain(
            schema::Chain {
                family: types::NfFamily::IP,
                table: Cow::Borrowed("rk8s"),
                name: Cow::Borrowed(name),
                ..Default::default()
            },
        )));
    }

    // 5. Base Rules (Jumps)
    let jumps = vec![
        ("nat-prerouting", "services"),
        ("nat-output", "services"),
        ("nat-postrouting", "masquerade"),
    ];
    for (chain, target) in jumps {
        objects.push(schema::NfObject::ListObject(schema::NfListObject::Rule(
            schema::Rule {
                family: types::NfFamily::IP,
                table: Cow::Borrowed("rk8s"),
                chain: Cow::Borrowed(chain),
                expr: Cow::Owned(vec![stmt::Statement::Jump(stmt::JumpTarget {
                    target: Cow::Borrowed(target),
                })]),
                ..Default::default()
            },
        )));
    }

    // 6. Dispatch Rules in `services` chain
    // TCP
    objects.push(schema::NfObject::ListObject(schema::NfListObject::Rule(
        schema::Rule {
            family: types::NfFamily::IP,
            table: Cow::Borrowed("rk8s"),
            chain: Cow::Borrowed("services"),
            expr: Cow::Owned(vec![
                stmt::Statement::Match(stmt::Match {
                    left: expr::Expression::Named(expr::NamedExpression::Meta(expr::Meta {
                        key: expr::MetaKey::L4proto,
                    })),
                    op: stmt::Operator::EQ,
                    right: expr::Expression::String(Cow::Borrowed("tcp")),
                }),
                stmt::Statement::Jump(stmt::JumpTarget {
                    target: Cow::Borrowed("services_tcp"),
                }),
            ]),
            ..Default::default()
        },
    )));
    // UDP
    objects.push(schema::NfObject::ListObject(schema::NfListObject::Rule(
        schema::Rule {
            family: types::NfFamily::IP,
            table: Cow::Borrowed("rk8s"),
            chain: Cow::Borrowed("services"),
            expr: Cow::Owned(vec![
                stmt::Statement::Match(stmt::Match {
                    left: expr::Expression::Named(expr::NamedExpression::Meta(expr::Meta {
                        key: expr::MetaKey::L4proto,
                    })),
                    op: stmt::Operator::EQ,
                    right: expr::Expression::String(Cow::Borrowed("udp")),
                }),
                stmt::Statement::Jump(stmt::JumpTarget {
                    target: Cow::Borrowed("services_udp"),
                }),
            ]),
            ..Default::default()
        },
    )));

    // 7. Masquerade Rules (Granular policies)
    // Scenario 1: Pod → ClusterIP → Other Pod (identified by the service mark)
    // When traffic is marked, it indicates Pod-to-Pod traffic via Service, requiring SNAT
    objects.push(schema::NfObject::ListObject(schema::NfListObject::Rule(
        schema::Rule {
            family: types::NfFamily::IP,
            table: Cow::Borrowed("rk8s"),
            chain: Cow::Borrowed("masquerade"),
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

    // 8. Generate Service Chains (Full Sync)
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
        let update_objects = generate_service_update_objects(svc, &ep, true)?;
        objects.extend(update_objects);
    }

    let nftables = schema::Nftables {
        objects: Cow::Owned(objects),
    };
    serde_json::to_string(&nftables).map_err(|e| anyhow::anyhow!(e))
}

fn generate_service_update_objects(
    svc: &common::ServiceTask,
    ep: &common::Endpoint,
    full_sync: bool,
) -> Result<Vec<schema::NfObject<'static>>> {
    let cluster_ip = match svc.spec.cluster_ip.as_deref() {
        Some(ip) if ip != "None" && !ip.is_empty() => ip,
        _ => return Ok(Vec::new()),
    };

    let mut objects = Vec::new();

    for svc_port in &svc.spec.ports {
        let protocol = svc_port.protocol.to_lowercase();
        let chain_name = format!(
            "svc-{}-{}-{}",
            svc.metadata.namespace, svc.metadata.name, svc_port.port
        );
        let dispatch_chain = if protocol == "udp" {
            "services_udp"
        } else {
            "services_tcp"
        };

        if !full_sync {
            // Delete old dispatch rule for ClusterIP to avoid duplicates (Incremental only)
            let delete_clusterip_rule = create_dispatch_rule(
                dispatch_chain,
                &protocol,
                cluster_ip,
                svc_port.port,
                &chain_name,
            );
            objects.push(schema::NfObject::CmdObject(schema::NfCmd::Delete(
                schema::NfListObject::Rule(delete_clusterip_rule),
            )));

            // Delete old dispatch rule for NodePort if exists (Incremental only)
            if let Some(node_port) = svc_port.node_port {
                let delete_nodeport_rule = create_dispatch_rule(
                    dispatch_chain,
                    &protocol,
                    "0.0.0.0/0",
                    node_port,
                    &chain_name,
                );
                objects.push(schema::NfObject::CmdObject(schema::NfCmd::Delete(
                    schema::NfListObject::Rule(delete_nodeport_rule),
                )));
            }
        }

        // 1. Create Chain
        objects.push(schema::NfObject::ListObject(schema::NfListObject::Chain(
            schema::Chain {
                family: types::NfFamily::IP,
                table: Cow::Borrowed("rk8s"),
                name: Cow::Owned(chain_name.clone()),
                ..Default::default()
            },
        )));

        // 3. Flush & Delete Chain
        objects.push(schema::NfObject::CmdObject(schema::NfCmd::Flush(
            schema::FlushObject::Chain(schema::Chain {
                family: types::NfFamily::IP,
                table: Cow::Borrowed("rk8s"),
                name: Cow::Owned(chain_name.clone()),
                ..Default::default()
            }),
        )));

        // 3. Add Dispatch Rule (in services_tcp/udp)
        // Mark ClusterIP traffic for SNAT identification in postrouting
        objects.push(schema::NfObject::ListObject(schema::NfListObject::Rule(
            schema::Rule {
                family: types::NfFamily::IP,
                table: Cow::Borrowed("rk8s"),
                chain: Cow::Borrowed(dispatch_chain),
                expr: Cow::Owned(vec![
                    stmt::Statement::Match(stmt::Match {
                        left: expr::Expression::Named(expr::NamedExpression::Meta(expr::Meta {
                            key: expr::MetaKey::L4proto,
                        })),
                        op: stmt::Operator::EQ,
                        right: expr::Expression::String(Cow::Owned(protocol.clone())),
                    }),
                    stmt::Statement::Match(stmt::Match {
                        left: expr::Expression::Named(expr::NamedExpression::Payload(
                            expr::Payload::PayloadField(expr::PayloadField {
                                protocol: Cow::Borrowed("ip"),
                                field: Cow::Borrowed("daddr"),
                            }),
                        )),
                        op: stmt::Operator::EQ,
                        right: expr::Expression::String(Cow::Owned(cluster_ip.to_string())),
                    }),
                    stmt::Statement::Match(stmt::Match {
                        left: expr::Expression::Named(expr::NamedExpression::Payload(
                            expr::Payload::PayloadField(expr::PayloadField {
                                protocol: Cow::Owned(protocol.clone()),
                                field: Cow::Borrowed("dport"),
                            }),
                        )),
                        op: stmt::Operator::EQ,
                        right: expr::Expression::Number(svc_port.port as u32),
                    }),
                    // Mark ClusterIP path for postrouting SNAT (NodePort path won't have this mark)
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

        // 4. Build Backends
        let mut backends = Vec::new();
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
                    backends.push((addr.ip.clone(), tp.port));
                }
            }
        }

        // 5. Generate Rules in svc chain
        if backends.is_empty() {
            // Reject
            objects.push(schema::NfObject::ListObject(schema::NfListObject::Rule(
                schema::Rule {
                    family: types::NfFamily::IP,
                    table: Cow::Borrowed("rk8s"),
                    chain: Cow::Owned(chain_name.clone()),
                    expr: Cow::Owned(vec![
                        stmt::Statement::Reject(None), // Reject with default type (icmp port-unreachable)
                    ]),
                    comment: Some(Cow::Borrowed("Reject (no endpoints)")),
                    ..Default::default()
                },
            )));
        } else {
            // Load Balancing
            let num_backends = backends.len() as u32;

            if num_backends > 1 {
                // 1. Set backend index for load balancing (temporarily overwrites the service mark, restored after selection)
                objects.push(schema::NfObject::ListObject(schema::NfListObject::Rule(
                    schema::Rule {
                        family: types::NfFamily::IP,
                        table: Cow::Borrowed("rk8s"),
                        chain: Cow::Owned(chain_name.clone()),
                        expr: Cow::Owned(vec![stmt::Statement::Mangle(stmt::Mangle {
                            key: expr::Expression::Named(expr::NamedExpression::Meta(expr::Meta {
                                key: expr::MetaKey::Mark,
                            })),
                            value: expr::Expression::Named(expr::NamedExpression::Numgen(
                                expr::Numgen {
                                    mode: expr::NgMode::Random,
                                    ng_mod: num_backends,
                                    offset: Some(0),
                                },
                            )),
                        })]),
                        comment: Some(Cow::Borrowed("LB: set backend index")),
                        ..Default::default()
                    },
                )));

                // 2. Dispatch to backends based on index (service mark already set by dispatch rule)
                for (i, (ip, port)) in backends.iter().enumerate() {
                    objects.push(schema::NfObject::ListObject(schema::NfListObject::Rule(
                        schema::Rule {
                            family: types::NfFamily::IP,
                            table: Cow::Borrowed("rk8s"),
                            chain: Cow::Owned(chain_name.clone()),
                            expr: Cow::Owned(vec![
                                // Ensure l4proto is matched before DNAT with port
                                stmt::Statement::Match(stmt::Match {
                                    left: expr::Expression::Named(expr::NamedExpression::Meta(
                                        expr::Meta {
                                            key: expr::MetaKey::L4proto,
                                        },
                                    )),
                                    op: stmt::Operator::EQ,
                                    right: expr::Expression::String(Cow::Owned(protocol.clone())),
                                }),
                                // Match backend index
                                stmt::Statement::Match(stmt::Match {
                                    left: expr::Expression::Named(expr::NamedExpression::Meta(
                                        expr::Meta {
                                            key: expr::MetaKey::Mark,
                                        },
                                    )),
                                    op: stmt::Operator::EQ,
                                    right: expr::Expression::Number(i as u32),
                                }),
                                // Restore service mark for masquerade identification in postrouting
                                stmt::Statement::Mangle(stmt::Mangle {
                                    key: expr::Expression::Named(expr::NamedExpression::Meta(
                                        expr::Meta {
                                            key: expr::MetaKey::Mark,
                                        },
                                    )),
                                    value: expr::Expression::Number(SERVICE_TRAFFIC_MARK),
                                }),
                                // Perform DNAT
                                stmt::Statement::DNAT(Some(stmt::NAT {
                                    addr: Some(expr::Expression::String(Cow::Owned(ip.clone()))),
                                    family: Some(stmt::NATFamily::IP),
                                    port: Some(expr::Expression::Number(*port as u32)),
                                    flags: None,
                                })),
                            ]),
                            ..Default::default()
                        },
                    )));
                }
            } else {
                // Single backend (mark already set by ClusterIP dispatch rule, preserved for masquerade)
                let (ip, port) = &backends[0];
                objects.push(schema::NfObject::ListObject(schema::NfListObject::Rule(
                    schema::Rule {
                        family: types::NfFamily::IP,
                        table: Cow::Borrowed("rk8s"),
                        chain: Cow::Owned(chain_name.clone()),
                        expr: Cow::Owned(vec![
                            // Ensure l4proto is matched before DNAT with port
                            stmt::Statement::Match(stmt::Match {
                                left: expr::Expression::Named(expr::NamedExpression::Meta(
                                    expr::Meta {
                                        key: expr::MetaKey::L4proto,
                                    },
                                )),
                                op: stmt::Operator::EQ,
                                right: expr::Expression::String(Cow::Owned(protocol.clone())),
                            }),
                            // Perform DNAT (mark already set by ClusterIP dispatch rule)
                            stmt::Statement::DNAT(Some(stmt::NAT {
                                addr: Some(expr::Expression::String(Cow::Owned(ip.clone()))),
                                family: Some(stmt::NATFamily::IP),
                                port: Some(expr::Expression::Number(*port as u32)),
                                flags: None,
                            })),
                        ]),
                        ..Default::default()
                    },
                )));
            }
        }

        // 6. NodePort Logic
        if let Some(node_port) = svc_port.node_port {
            objects.push(schema::NfObject::ListObject(schema::NfListObject::Rule(
                schema::Rule {
                    family: types::NfFamily::IP,
                    table: Cow::Borrowed("rk8s"),
                    chain: Cow::Borrowed(dispatch_chain),
                    expr: Cow::Owned(vec![
                        // ensure transport protocol is matched before accessing dport
                        stmt::Statement::Match(stmt::Match {
                            left: expr::Expression::Named(expr::NamedExpression::Meta(
                                expr::Meta {
                                    key: expr::MetaKey::L4proto,
                                },
                            )),
                            op: stmt::Operator::EQ,
                            right: expr::Expression::String(Cow::Owned(protocol.clone())),
                        }),
                        stmt::Statement::Match(stmt::Match {
                            left: expr::Expression::Named(expr::NamedExpression::Payload(
                                expr::Payload::PayloadField(expr::PayloadField {
                                    protocol: Cow::Owned(protocol.clone()),
                                    field: Cow::Borrowed("dport"),
                                }),
                            )),
                            op: stmt::Operator::EQ,
                            right: expr::Expression::Number(node_port as u32),
                        }),
                        stmt::Statement::Jump(stmt::JumpTarget {
                            target: Cow::Owned(chain_name.clone()),
                        }),
                    ]),
                    ..Default::default()
                },
            )));
        }
    }

    Ok(objects)
}

pub fn generate_service_update(svc: &common::ServiceTask, ep: &common::Endpoint) -> Result<String> {
    let objects = generate_service_update_objects(svc, ep, false)?;
    let nftables = schema::Nftables {
        objects: Cow::Owned(objects),
    };
    serde_json::to_string(&nftables).map_err(|e| anyhow::anyhow!(e))
}

pub fn generate_service_delete(svc: &common::ServiceTask) -> Result<String> {
    let cluster_ip = match svc.spec.cluster_ip.as_deref() {
        Some(ip) => ip,
        None => return Ok(json!({"nftables": []}).to_string()),
    };

    let mut objects = Vec::new();

    for svc_port in &svc.spec.ports {
        let protocol = svc_port.protocol.to_lowercase();
        let chain_name = format!(
            "svc-{}-{}-{}",
            svc.metadata.namespace, svc.metadata.name, svc_port.port
        );
        let dispatch_chain = if protocol == "udp" {
            "services_udp"
        } else {
            "services_tcp"
        };

        // 1. Delete ClusterIP Dispatch Rule (must match exactly including mark statement)
        let rule = schema::Rule {
            family: types::NfFamily::IP,
            table: Cow::Borrowed("rk8s"),
            chain: Cow::Borrowed(dispatch_chain),
            expr: Cow::Owned(vec![
                stmt::Statement::Match(stmt::Match {
                    left: expr::Expression::Named(expr::NamedExpression::Meta(expr::Meta {
                        key: expr::MetaKey::L4proto,
                    })),
                    op: stmt::Operator::EQ,
                    right: expr::Expression::String(Cow::Owned(protocol.clone())),
                }),
                stmt::Statement::Match(stmt::Match {
                    left: expr::Expression::Named(expr::NamedExpression::Payload(
                        expr::Payload::PayloadField(expr::PayloadField {
                            protocol: Cow::Borrowed("ip"),
                            field: Cow::Borrowed("daddr"),
                        }),
                    )),
                    op: stmt::Operator::EQ,
                    right: expr::Expression::String(Cow::Owned(cluster_ip.to_string())),
                }),
                stmt::Statement::Match(stmt::Match {
                    left: expr::Expression::Named(expr::NamedExpression::Payload(
                        expr::Payload::PayloadField(expr::PayloadField {
                            protocol: Cow::Owned(protocol.clone()),
                            field: Cow::Borrowed("dport"),
                        }),
                    )),
                    op: stmt::Operator::EQ,
                    right: expr::Expression::Number(svc_port.port as u32),
                }),
                // Include mark statement to match creation rule exactly
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
        };
        objects.push(schema::NfObject::CmdObject(schema::NfCmd::Delete(
            schema::NfListObject::Rule(rule),
        )));

        // 2. Delete NodePort Rule
        if let Some(node_port) = svc_port.node_port {
            let np_rule = schema::Rule {
                family: types::NfFamily::IP,
                table: Cow::Borrowed("rk8s"),
                chain: Cow::Borrowed(dispatch_chain),
                expr: Cow::Owned(vec![
                    // ensure l4proto match to match created NodePort rule
                    stmt::Statement::Match(stmt::Match {
                        left: expr::Expression::Named(expr::NamedExpression::Meta(expr::Meta {
                            key: expr::MetaKey::L4proto,
                        })),
                        op: stmt::Operator::EQ,
                        right: expr::Expression::String(Cow::Owned(protocol.clone())),
                    }),
                    stmt::Statement::Match(stmt::Match {
                        left: expr::Expression::Named(expr::NamedExpression::Payload(
                            expr::Payload::PayloadField(expr::PayloadField {
                                protocol: Cow::Owned(protocol.clone()),
                                field: Cow::Borrowed("dport"),
                            }),
                        )),
                        op: stmt::Operator::EQ,
                        right: expr::Expression::Number(node_port as u32),
                    }),
                    stmt::Statement::Jump(stmt::JumpTarget {
                        target: Cow::Owned(chain_name.clone()),
                    }),
                ]),
                ..Default::default()
            };
            objects.push(schema::NfObject::CmdObject(schema::NfCmd::Delete(
                schema::NfListObject::Rule(np_rule),
            )));
        }

        // 3. Flush & Delete Chain
        objects.push(schema::NfObject::CmdObject(schema::NfCmd::Flush(
            schema::FlushObject::Chain(schema::Chain {
                family: types::NfFamily::IP,
                table: Cow::Borrowed("rk8s"),
                name: Cow::Owned(chain_name.clone()),
                ..Default::default()
            }),
        )));
        objects.push(schema::NfObject::CmdObject(schema::NfCmd::Delete(
            schema::NfListObject::Chain(schema::Chain {
                family: types::NfFamily::IP,
                table: Cow::Borrowed("rk8s"),
                name: Cow::Owned(chain_name.clone()),
                ..Default::default()
            }),
        )));
    }

    let nftables = schema::Nftables {
        objects: Cow::Owned(objects),
    };
    serde_json::to_string(&nftables).map_err(|e| anyhow::anyhow!(e))
}

/// Helper function to create a dispatch rule for ClusterIP or NodePort
/// Used for both adding and deleting rules in services_tcp/udp chains
fn create_dispatch_rule<'a>(
    chain: &'a str,
    protocol: &str,
    dst_ip: &str,
    dst_port: i32,
    target_chain: &str,
) -> schema::Rule<'a> {
    schema::Rule {
        family: types::NfFamily::IP,
        table: Cow::Borrowed("rk8s"),
        chain: Cow::Borrowed(chain),
        expr: Cow::Owned(vec![
            // Match transport protocol
            stmt::Statement::Match(stmt::Match {
                left: expr::Expression::Named(expr::NamedExpression::Meta(expr::Meta {
                    key: expr::MetaKey::L4proto,
                })),
                op: stmt::Operator::EQ,
                right: expr::Expression::String(Cow::Owned(protocol.to_string())),
            }),
            // Match destination IP
            stmt::Statement::Match(stmt::Match {
                left: expr::Expression::Named(expr::NamedExpression::Payload(
                    expr::Payload::PayloadField(expr::PayloadField {
                        protocol: Cow::Borrowed("ip"),
                        field: Cow::Borrowed("daddr"),
                    }),
                )),
                op: stmt::Operator::EQ,
                right: expr::Expression::String(Cow::Owned(dst_ip.to_string())),
            }),
            // Match destination port
            stmt::Statement::Match(stmt::Match {
                left: expr::Expression::Named(expr::NamedExpression::Payload(
                    expr::Payload::PayloadField(expr::PayloadField {
                        protocol: Cow::Owned(protocol.to_string()),
                        field: Cow::Borrowed("dport"),
                    }),
                )),
                op: stmt::Operator::EQ,
                right: expr::Expression::Number(dst_port as u32),
            }),
            // Jump to service chain
            stmt::Statement::Jump(stmt::JumpTarget {
                target: Cow::Owned(target_chain.to_string()),
            }),
        ]),
        ..Default::default()
    }
}
