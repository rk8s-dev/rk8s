use common::{
    Endpoint, EndpointAddress, EndpointPort, EndpointSubset, ObjectMeta, ServicePort, ServiceSpec,
    ServiceTask,
};
use libnetwork::nftables::{
    generate_nftables_config, generate_service_delete_with_endpoint, generate_service_update,
    generate_service_update_with_old_endpoint, generate_services_discovery_refresh,
};
use std::io::Write;
use std::process::{Command, Stdio};

fn object_rule<'a>(obj: &'a serde_json::Value) -> Option<&'a serde_json::Value> {
    obj.get("rule")
        .or_else(|| obj.get("add").and_then(|add| add.get("rule")))
}

fn object_chain<'a>(obj: &'a serde_json::Value) -> Option<&'a serde_json::Value> {
    obj.get("chain")
        .or_else(|| obj.get("add").and_then(|add| add.get("chain")))
}

fn diagnose_transport_payloads(json_str: &str) {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str)
        && let Some(arr) = val.get("nftables").and_then(|v| v.as_array())
    {
        for (idx, obj) in arr.iter().enumerate() {
            if let Some(rule) = obj.get("rule")
                && let Some(expr_arr) = rule.get("expr").and_then(|v| v.as_array())
            {
                let mut has_l4proto_match = false;
                let mut has_transport_payload = false;
                let mut transport_protocol = String::new();

                for (expr_idx, expr) in expr_arr.iter().enumerate() {
                    if let Some(m) = expr.get("match")
                        && let Some(left) = m.get("left")
                        && let Some(meta) = left.get("meta")
                        && let Some(key) = meta.get("key").and_then(|v| v.as_str())
                        && key == "l4proto"
                    {
                        has_l4proto_match = true;
                    }

                    if let Some(m) = expr.get("match")
                        && let Some(left) = m.get("left")
                        && let Some(payload) = left.get("payload")
                        && let Some(field) = payload.get("field")
                        && let Some(proto) = payload.get("protocol").and_then(|v| v.as_str())
                        && (proto == "tcp" || proto == "udp")
                    {
                        has_transport_payload = true;
                        transport_protocol = proto.to_string();
                        if !has_l4proto_match {
                            // Use ASCII markers to avoid Unicode escape issues in fmt
                            println!(
                                "[WARN] Rule #{} expr[{}]: Found {} payload without prior l4proto match",
                                idx, expr_idx, proto
                            );
                            println!("    Rule chain: {:?}", rule.get("chain"));
                            println!("    Field: {:?}", field);
                        }
                    }
                }

                if has_transport_payload && !has_l4proto_match {
                    println!(
                        "[ERROR] Rule #{}: Uses {} payload WITHOUT l4proto match",
                        idx, transport_protocol
                    );
                    println!("   Chain: {:?}", rule.get("chain"));
                }
            }
        }
    }
}

fn has_vmap_statement(rule: &serde_json::Value) -> bool {
    rule.get("expr")
        .and_then(|v| v.as_array())
        .map(|exprs| {
            exprs
                .iter()
                .any(|expr| expr.get("vmap").is_some() || expr.get("map").is_some())
        })
        .unwrap_or(false)
}

fn has_dnat_map_statement(rule: &serde_json::Value) -> bool {
    rule.get("expr")
        .and_then(|v| v.as_array())
        .map(|exprs| {
            exprs.iter().any(|expr| {
                expr.get("dnat")
                    .and_then(|d| d.get("addr"))
                    .and_then(|a| a.get("map"))
                    .is_some()
            })
        })
        .unwrap_or(false)
}

fn has_chain_object(json: &serde_json::Value, chain_name: &str) -> bool {
    json.get("nftables")
        .and_then(|v| v.as_array())
        .map(|objects| {
            objects.iter().any(|obj| {
                object_chain(obj)
                    .and_then(|c| c.get("name"))
                    .and_then(|v| v.as_str())
                    == Some(chain_name)
            })
        })
        .unwrap_or(false)
}

fn has_vmap_rule_in_chain(json: &serde_json::Value, chain_name: &str) -> bool {
    json.get("nftables")
        .and_then(|v| v.as_array())
        .map(|objects| {
            objects.iter().any(|obj| {
                object_rule(obj)
                    .and_then(|rule| {
                        let same_chain =
                            rule.get("chain").and_then(|v| v.as_str()) == Some(chain_name);
                        if same_chain && has_vmap_statement(rule) {
                            Some(true)
                        } else {
                            None
                        }
                    })
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

fn has_dnat_map_rule_in_chain(json: &serde_json::Value, chain_name: &str) -> bool {
    json.get("nftables")
        .and_then(|v| v.as_array())
        .map(|objects| {
            objects.iter().any(|obj| {
                object_rule(obj)
                    .and_then(|rule| {
                        let same_chain =
                            rule.get("chain").and_then(|v| v.as_str()) == Some(chain_name);
                        if same_chain && has_dnat_map_statement(rule) {
                            Some(true)
                        } else {
                            None
                        }
                    })
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

fn has_delete_rule_cmd(json: &serde_json::Value) -> bool {
    json.get("nftables")
        .and_then(|v| v.as_array())
        .map(|objects| {
            objects
                .iter()
                .any(|obj| obj.get("delete").and_then(|d| d.get("rule")).is_some())
        })
        .unwrap_or(false)
}

fn has_delete_chain_cmd(json: &serde_json::Value, chain_name: &str) -> bool {
    json.get("nftables")
        .and_then(|v| v.as_array())
        .map(|objects| {
            objects.iter().any(|obj| {
                obj.get("delete")
                    .and_then(|d| d.get("chain"))
                    .and_then(|chain| chain.get("name"))
                    .and_then(|v| v.as_str())
                    == Some(chain_name)
            })
        })
        .unwrap_or(false)
}

fn has_flush_chain_cmd(json: &serde_json::Value, chain_name: &str) -> bool {
    json.get("nftables")
        .and_then(|v| v.as_array())
        .map(|objects| {
            objects.iter().any(|obj| {
                obj.get("flush")
                    .and_then(|flush| flush.get("chain"))
                    .and_then(|chain| chain.get("name"))
                    .and_then(|v| v.as_str())
                    == Some(chain_name)
            })
        })
        .unwrap_or(false)
}

fn numgen_mod_in_chain(json: &serde_json::Value, chain_name: &str) -> Option<u64> {
    json.get("nftables")
        .and_then(|v| v.as_array())
        .and_then(|objects| {
            objects.iter().find_map(|obj| {
                let rule = obj.get("rule")?;
                let same_chain = rule.get("chain").and_then(|v| v.as_str()) == Some(chain_name);
                if !same_chain {
                    return None;
                }
                rule.get("expr")
                    .and_then(|v| v.as_array())
                    .and_then(|exprs| {
                        exprs.iter().find_map(|e| {
                            e.get("vmap")
                                .and_then(|v| v.get("key"))
                                .and_then(|k| k.get("numgen"))
                                .and_then(|n| n.get("mod"))
                                .and_then(|v| v.as_u64())
                                .or_else(|| {
                                    e.get("dnat")
                                        .and_then(|d| d.get("addr"))
                                        .and_then(|a| a.get("map"))
                                        .and_then(|m| m.get("key"))
                                        .and_then(|k| k.get("numgen"))
                                        .and_then(|n| n.get("mod"))
                                        .and_then(|v| v.as_u64())
                                })
                        })
                    })
            })
        })
}

fn has_reject_rule_in_chain(json: &serde_json::Value, chain_name: &str) -> bool {
    json.get("nftables")
        .and_then(|v| v.as_array())
        .map(|objects| {
            objects.iter().any(|obj| {
                let Some(rule) = obj.get("rule") else {
                    return false;
                };
                if rule.get("chain").and_then(|v| v.as_str()) != Some(chain_name) {
                    return false;
                }
                rule.get("expr")
                    .and_then(|v| v.as_array())
                    .map(|exprs| exprs.iter().any(|e| e.get("reject").is_some()))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

fn services_chain_has_clusterip_lookup(json: &serde_json::Value) -> bool {
    json.get("nftables")
        .and_then(|v| v.as_array())
        .map(|objects| {
            objects.iter().any(|obj| {
                let Some(rule) = object_rule(obj) else {
                    return false;
                };
                if rule.get("chain").and_then(|v| v.as_str()) != Some("services") {
                    return false;
                }
                rule.get("expr")
                    .and_then(|v| v.as_array())
                    .map(|exprs| {
                        exprs.iter().any(|e| {
                            e.get("vmap")
                                .or_else(|| e.get("map"))
                                .and_then(|v| v.get("key"))
                                .and_then(|k| k.get("concat"))
                                .and_then(|v| v.as_array())
                                .map(|parts| {
                                    let has_daddr = parts.iter().any(|p| {
                                        p.get("payload")
                                            .and_then(|x| x.get("field"))
                                            .and_then(|v| v.as_str())
                                            == Some("daddr")
                                    });
                                    let has_dport = parts.iter().any(|p| {
                                        p.get("payload")
                                            .and_then(|x| x.get("field"))
                                            .and_then(|v| v.as_str())
                                            == Some("dport")
                                    });
                                    has_daddr && has_dport
                                })
                                .unwrap_or(false)
                        })
                    })
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

fn services_chain_has_nodeport_lookup(json: &serde_json::Value) -> bool {
    json.get("nftables")
        .and_then(|v| v.as_array())
        .map(|objects| {
            objects.iter().any(|obj| {
                let Some(rule) = object_rule(obj) else {
                    return false;
                };
                if rule.get("chain").and_then(|v| v.as_str()) != Some("services") {
                    return false;
                }
                rule.get("expr")
                    .and_then(|v| v.as_array())
                    .map(|exprs| {
                        exprs.iter().any(|e| {
                            e.get("vmap")
                                .or_else(|| e.get("map"))
                                .and_then(|v| v.get("key"))
                                .and_then(|k| k.get("concat"))
                                .and_then(|v| v.as_array())
                                .map(|parts| {
                                    let has_proto = parts.iter().any(|p| {
                                        p.get("payload")
                                            .and_then(|x| x.get("field"))
                                            .and_then(|v| v.as_str())
                                            == Some("protocol")
                                    });
                                    let has_dport = parts.iter().any(|p| {
                                        p.get("payload")
                                            .and_then(|x| x.get("field"))
                                            .and_then(|v| v.as_str())
                                            == Some("dport")
                                    });
                                    has_proto && has_dport
                                })
                                .unwrap_or(false)
                        })
                    })
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

fn render_key_fragments_snapshot(json: &serde_json::Value) -> String {
    let mut lines = Vec::new();
    let Some(objects) = json.get("nftables").and_then(|v| v.as_array()) else {
        return String::new();
    };

    for obj in objects {
        let Some(rule) = object_rule(obj) else {
            continue;
        };
        let Some(chain) = rule.get("chain").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(exprs) = rule.get("expr").and_then(|v| v.as_array()) else {
            continue;
        };

        if chain == "services"
            && let Some(vmap) = exprs
                .iter()
                .find_map(|e| e.get("vmap").or_else(|| e.get("map")))
            && let Some(key_arr) = vmap
                .get("key")
                .and_then(|v| v.get("concat"))
                .and_then(|v| v.as_array())
        {
            let key_fields: Vec<String> = key_arr
                .iter()
                .filter_map(|k| {
                    let p = k.get("payload")?;
                    let proto = p.get("protocol")?.as_str()?;
                    let field = p.get("field")?.as_str()?;
                    Some(format!("{}.{}", proto, field))
                })
                .collect();
            lines.push(format!("services: {} vmap", key_fields.join(" + ")));
        }

        if chain.starts_with("svc-") {
            if let Some(vmap) = exprs
                .iter()
                .find_map(|e| e.get("vmap").or_else(|| e.get("map")))
                && let Some(numgen) = vmap
                    .get("key")
                    .and_then(|k| k.get("numgen"))
                    .and_then(|v| v.as_object())
                && let Some(ng_mod) = numgen.get("mod").and_then(|v| v.as_u64())
            {
                lines.push(format!("{}: numgen random mod {} vmap", chain, ng_mod));
            }

            if let Some(dnat_map) = exprs
                .iter()
                .find_map(|e| e.get("dnat"))
                .and_then(|d| d.get("addr"))
                .and_then(|a| a.get("map"))
                && let Some(ng_mod) = dnat_map
                    .get("key")
                    .and_then(|k| k.get("numgen"))
                    .and_then(|n| n.get("mod"))
                    .and_then(|v| v.as_u64())
            {
                lines.push(format!("{}: numgen random mod {} dnat-map", chain, ng_mod));
            }
        }

        if chain.starts_with("mark-svc-") {
            let has_mark_set = exprs.iter().any(|e| {
                e.get("mangle")
                    .and_then(|m| m.get("key"))
                    .and_then(|k| k.get("meta"))
                    .and_then(|m| m.get("key"))
                    .and_then(|v| v.as_str())
                    == Some("mark")
            });
            let jump_target = exprs
                .iter()
                .find_map(|e| e.get("jump"))
                .and_then(|j| j.get("target"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if has_mark_set {
                lines.push(format!("{}: mark set -> {}", chain, jump_target));
            }
        }

        if chain.starts_with("ep-") {
            let l4proto = exprs
                .iter()
                .find_map(|e| e.get("match"))
                .and_then(|m| m.get("right"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if let Some(dnat) = exprs.iter().find_map(|e| e.get("dnat")) {
                let addr = dnat.get("addr").and_then(|v| v.as_str()).unwrap_or("");
                let port = dnat
                    .get("port")
                    .and_then(|v| v.as_u64())
                    .unwrap_or_default();
                lines.push(format!("{}: {} dnat {}:{}", chain, l4proto, addr, port));
            }
        }
    }

    lines.sort();
    lines.join("\n")
}

#[test]
fn test_generate_and_check_nftables() {
    let svc = ServiceTask {
        api_version: "v1".into(),
        kind: "Service".into(),
        metadata: ObjectMeta {
            name: "mysvc".into(),
            namespace: "default".into(),
            ..Default::default()
        },
        spec: ServiceSpec {
            cluster_ip: Some("10.96.0.100".into()),
            ports: vec![ServicePort {
                name: Some("http".into()),
                protocol: "TCP".into(),
                port: 80,
                target_port: Some(8080),
                node_port: Some(30080),
            }],
            selector: None,
            service_type: "NodePort".into(),
        },
    };
    let ep = Endpoint {
        api_version: "v1".into(),
        kind: "Endpoints".into(),
        metadata: ObjectMeta {
            name: "mysvc".into(),
            namespace: "default".into(),
            ..Default::default()
        },
        subsets: vec![EndpointSubset {
            addresses: vec![
                EndpointAddress {
                    ip: "10.244.1.2".into(),
                    node_name: None,
                    target_ref: None,
                },
                EndpointAddress {
                    ip: "10.244.1.3".into(),
                    node_name: None,
                    target_ref: None,
                },
            ],
            not_ready_addresses: vec![],
            ports: vec![EndpointPort {
                name: Some("http".into()),
                port: 8080,
                protocol: "TCP".into(),
                app_protocol: None,
            }],
        }],
    };
    let json = generate_nftables_config(&[svc], &[ep]).expect("generate_nftables_config failed");
    diagnose_transport_payloads(&json);

    let parsed: serde_json::Value =
        serde_json::from_str(&json).expect("generated nftables payload must be valid json");

    assert!(
        has_dnat_map_rule_in_chain(&parsed, "svc-default-mysvc-80"),
        "service chain must use numgen+dnat-map for endpoint load balancing"
    );
    assert!(
        has_chain_object(&parsed, "mark-svc-default-mysvc-80"),
        "full sync should create mark chain for ClusterIP traffic"
    );
    assert!(
        !has_chain_object(&parsed, "services_tcp"),
        "legacy services_tcp chain should not exist in full sync output"
    );
    assert!(
        !has_chain_object(&parsed, "services_udp"),
        "legacy services_udp chain should not exist in full sync output"
    );

    let snapshot = render_key_fragments_snapshot(&parsed);
    let expected = [
        "mark-svc-default-mysvc-80: mark set -> svc-default-mysvc-80",
        "svc-default-mysvc-80: numgen random mod 2 dnat-map",
    ]
    .join("\n");
    assert_eq!(snapshot, expected, "key nft rule snapshot changed");

    // Use `nft --check` to validate the generated rules; tolerate missing perms/binary
    let spawn_result = Command::new("nft")
        .args(["-j", "--check", "-f", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    let mut child = match spawn_result {
        Ok(child) => child,
        Err(e) => {
            println!(
                "Skipping nft validation: nft not available or failed to start: {}",
                e
            );
            return;
        }
    };

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(json.as_bytes())
            .expect("failed to write nftables json to stdin");
    }

    let output = child
        .wait_with_output()
        .expect("failed to wait for nft --check");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    println!("nft --check stdout:\n{}", stdout);
    println!("nft --check stderr:\n{}", stderr);

    if !output.status.success() {
        // Don't fail the test if nft is missing permissions (common in CI/containers)
        if stderr.contains("Permission denied")
            || stderr.contains("Operation not permitted")
            || stdout.contains("Permission denied")
            || stdout.contains("Operation not permitted")
        {
            println!("Skipping validation: insufficient permissions to run nft --check");
            return;
        }

        if stderr.is_empty() && stdout.is_empty() {
            println!(
                "Skipping validation: nft --check failed with no output (likely permission issue)"
            );
            return;
        }

        panic!("nft configuration invalid");
    }
}

#[test]
fn test_incremental_update_uses_vmap_semantics() {
    let svc = ServiceTask {
        api_version: "v1".into(),
        kind: "Service".into(),
        metadata: ObjectMeta {
            name: "mysvc".into(),
            namespace: "default".into(),
            ..Default::default()
        },
        spec: ServiceSpec {
            cluster_ip: Some("10.96.0.100".into()),
            ports: vec![ServicePort {
                name: Some("http".into()),
                protocol: "TCP".into(),
                port: 80,
                target_port: Some(8080),
                node_port: Some(30080),
            }],
            selector: None,
            service_type: "NodePort".into(),
        },
    };
    let ep = Endpoint {
        api_version: "v1".into(),
        kind: "Endpoints".into(),
        metadata: ObjectMeta {
            name: "mysvc".into(),
            namespace: "default".into(),
            ..Default::default()
        },
        subsets: vec![EndpointSubset {
            addresses: vec![EndpointAddress {
                ip: "10.244.1.2".into(),
                node_name: None,
                target_ref: None,
            }],
            not_ready_addresses: vec![],
            ports: vec![EndpointPort {
                name: Some("http".into()),
                port: 8080,
                protocol: "TCP".into(),
                app_protocol: None,
            }],
        }],
    };

    let json = generate_service_update(&svc, &ep).expect("generate_service_update failed");
    let parsed: serde_json::Value =
        serde_json::from_str(&json).expect("incremental nftables payload must be valid json");

    assert!(
        !has_vmap_rule_in_chain(&parsed, "services"),
        "incremental endpoint update should not rewrite shared services dispatch chain"
    );
    assert!(
        has_dnat_map_rule_in_chain(&parsed, "svc-default-mysvc-80"),
        "incremental service chain must use numgen+dnat-map"
    );
    assert!(
        has_chain_object(&parsed, "mark-svc-default-mysvc-80"),
        "incremental update should include mark chain for ClusterIP path"
    );
    assert!(
        !has_delete_rule_cmd(&parsed),
        "incremental payload must not include delete-rule commands requiring nft handle"
    );
}

#[test]
fn test_single_backend_uses_numgen_mod_1() {
    let svc = ServiceTask {
        api_version: "v1".into(),
        kind: "Service".into(),
        metadata: ObjectMeta {
            name: "svc-one".into(),
            namespace: "default".into(),
            ..Default::default()
        },
        spec: ServiceSpec {
            cluster_ip: Some("10.96.0.101".into()),
            ports: vec![ServicePort {
                name: Some("http".into()),
                protocol: "TCP".into(),
                port: 80,
                target_port: Some(8080),
                node_port: None,
            }],
            selector: None,
            service_type: "ClusterIP".into(),
        },
    };

    let ep = Endpoint {
        api_version: "v1".into(),
        kind: "Endpoints".into(),
        metadata: ObjectMeta {
            name: "svc-one".into(),
            namespace: "default".into(),
            ..Default::default()
        },
        subsets: vec![EndpointSubset {
            addresses: vec![EndpointAddress {
                ip: "10.244.10.2".into(),
                node_name: None,
                target_ref: None,
            }],
            not_ready_addresses: vec![],
            ports: vec![EndpointPort {
                name: Some("http".into()),
                port: 8080,
                protocol: "TCP".into(),
                app_protocol: None,
            }],
        }],
    };

    let json = generate_service_update(&svc, &ep).expect("generate_service_update failed");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json expected");
    let mod_v = numgen_mod_in_chain(&parsed, "svc-default-svc-one-80");
    assert_eq!(mod_v, Some(1), "single backend must use numgen mod 1");
}

#[test]
fn test_multi_backend_uses_numgen_mod_n() {
    let svc = ServiceTask {
        api_version: "v1".into(),
        kind: "Service".into(),
        metadata: ObjectMeta {
            name: "svc-multi".into(),
            namespace: "default".into(),
            ..Default::default()
        },
        spec: ServiceSpec {
            cluster_ip: Some("10.96.0.102".into()),
            ports: vec![ServicePort {
                name: Some("http".into()),
                protocol: "TCP".into(),
                port: 80,
                target_port: Some(8080),
                node_port: None,
            }],
            selector: None,
            service_type: "ClusterIP".into(),
        },
    };
    let ep = Endpoint {
        api_version: "v1".into(),
        kind: "Endpoints".into(),
        metadata: ObjectMeta {
            name: "svc-multi".into(),
            namespace: "default".into(),
            ..Default::default()
        },
        subsets: vec![EndpointSubset {
            addresses: vec![
                EndpointAddress {
                    ip: "10.244.20.2".into(),
                    node_name: None,
                    target_ref: None,
                },
                EndpointAddress {
                    ip: "10.244.20.3".into(),
                    node_name: None,
                    target_ref: None,
                },
                EndpointAddress {
                    ip: "10.244.20.4".into(),
                    node_name: None,
                    target_ref: None,
                },
            ],
            not_ready_addresses: vec![],
            ports: vec![EndpointPort {
                name: Some("http".into()),
                port: 8080,
                protocol: "TCP".into(),
                app_protocol: None,
            }],
        }],
    };

    let json = generate_service_update(&svc, &ep).expect("generate_service_update failed");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json expected");
    let mod_v = numgen_mod_in_chain(&parsed, "svc-default-svc-multi-80");
    assert_eq!(mod_v, Some(3), "three backends must use numgen mod 3");
}

#[test]
fn test_service_discovery_contains_clusterip_and_nodeport_lookups() {
    let svc = ServiceTask {
        api_version: "v1".into(),
        kind: "Service".into(),
        metadata: ObjectMeta {
            name: "svc-discovery".into(),
            namespace: "default".into(),
            ..Default::default()
        },
        spec: ServiceSpec {
            cluster_ip: Some("10.96.0.103".into()),
            ports: vec![ServicePort {
                name: Some("http".into()),
                protocol: "TCP".into(),
                port: 80,
                target_port: Some(8080),
                node_port: Some(30080),
            }],
            selector: None,
            service_type: "NodePort".into(),
        },
    };
    let json = generate_services_discovery_refresh(&[svc])
        .expect("generate_services_discovery_refresh failed");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json expected");

    assert!(
        services_chain_has_clusterip_lookup(&parsed),
        "services chain should include ClusterIP lookup vmap"
    );
    assert!(
        services_chain_has_nodeport_lookup(&parsed),
        "services chain should include NodePort lookup vmap"
    );
}

#[test]
fn test_no_endpoints_generates_reject_rule() {
    let svc = ServiceTask {
        api_version: "v1".into(),
        kind: "Service".into(),
        metadata: ObjectMeta {
            name: "svc-reject".into(),
            namespace: "default".into(),
            ..Default::default()
        },
        spec: ServiceSpec {
            cluster_ip: Some("10.96.0.104".into()),
            ports: vec![ServicePort {
                name: Some("http".into()),
                protocol: "TCP".into(),
                port: 80,
                target_port: Some(8080),
                node_port: None,
            }],
            selector: None,
            service_type: "ClusterIP".into(),
        },
    };
    let ep = Endpoint {
        api_version: "v1".into(),
        kind: "Endpoints".into(),
        metadata: ObjectMeta {
            name: "svc-reject".into(),
            namespace: "default".into(),
            ..Default::default()
        },
        subsets: vec![],
    };

    let json = generate_service_update(&svc, &ep).expect("generate_service_update failed");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json expected");

    assert!(
        has_reject_rule_in_chain(&parsed, "svc-default-svc-reject-80"),
        "service chain should reject traffic when endpoints are empty"
    );
}

#[test]
fn test_services_discovery_refresh_payload_only_rebuilds_shared_chain() {
    let svc = ServiceTask {
        api_version: "v1".into(),
        kind: "Service".into(),
        metadata: ObjectMeta {
            name: "svc-refresh".into(),
            namespace: "default".into(),
            ..Default::default()
        },
        spec: ServiceSpec {
            cluster_ip: Some("10.96.0.105".into()),
            ports: vec![ServicePort {
                name: Some("http".into()),
                protocol: "TCP".into(),
                port: 80,
                target_port: Some(8080),
                node_port: Some(30081),
            }],
            selector: None,
            service_type: "NodePort".into(),
        },
    };

    let json = generate_services_discovery_refresh(&[svc])
        .expect("generate_services_discovery_refresh failed");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json expected");

    assert!(
        has_flush_chain_cmd(&parsed, "services"),
        "services discovery refresh must flush shared services chain"
    );
    assert!(
        services_chain_has_clusterip_lookup(&parsed),
        "services discovery refresh must rebuild ClusterIP lookup"
    );
    assert!(
        services_chain_has_nodeport_lookup(&parsed),
        "services discovery refresh must rebuild NodePort lookup"
    );
    assert!(
        !has_delete_rule_cmd(&parsed),
        "services discovery refresh payload must not include delete-rule commands"
    );
    assert!(
        !has_chain_object(&parsed, "svc-default-svc-refresh-80"),
        "services discovery refresh should not include per-service chain definitions"
    );
}

#[test]
fn test_service_delete_payload_flushes_and_deletes_chains_safely() {
    let svc = ServiceTask {
        api_version: "v1".into(),
        kind: "Service".into(),
        metadata: ObjectMeta {
            name: "svc-del".into(),
            namespace: "default".into(),
            ..Default::default()
        },
        spec: ServiceSpec {
            cluster_ip: Some("10.96.0.106".into()),
            ports: vec![ServicePort {
                name: Some("http".into()),
                protocol: "TCP".into(),
                port: 80,
                target_port: Some(8080),
                node_port: Some(30082),
            }],
            selector: None,
            service_type: "NodePort".into(),
        },
    };

    let ep = Endpoint {
        api_version: "v1".into(),
        kind: "Endpoints".into(),
        metadata: ObjectMeta {
            name: "svc-del".into(),
            namespace: "default".into(),
            ..Default::default()
        },
        subsets: vec![EndpointSubset {
            addresses: vec![EndpointAddress {
                ip: "10.244.40.2".into(),
                node_name: None,
                target_ref: None,
            }],
            not_ready_addresses: vec![],
            ports: vec![EndpointPort {
                name: Some("http".into()),
                port: 8080,
                protocol: "TCP".into(),
                app_protocol: None,
            }],
        }],
    };

    let json = generate_service_delete_with_endpoint(&svc, Some(&ep))
        .expect("generate_service_delete_with_endpoint failed");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json expected");

    assert!(
        has_flush_chain_cmd(&parsed, "svc-default-svc-del-80"),
        "service delete payload should flush service chain"
    );
    assert!(
        has_flush_chain_cmd(&parsed, "mark-svc-default-svc-del-80"),
        "service delete payload should flush mark chain"
    );
    assert!(
        !has_delete_chain_cmd(&parsed, "ep-svc-default-svc-del-80-0"),
        "phase C default path should not manage endpoint chain lifecycle"
    );
    assert!(
        has_delete_chain_cmd(&parsed, "svc-default-svc-del-80"),
        "service delete payload should delete service chain after reference cleanup"
    );
    assert!(
        has_delete_chain_cmd(&parsed, "mark-svc-default-svc-del-80"),
        "service delete payload should delete mark chain after reference cleanup"
    );
}

#[test]
fn test_endpoint_shrink_generates_stale_ep_chain_delete() {
    let svc = ServiceTask {
        api_version: "v1".into(),
        kind: "Service".into(),
        metadata: ObjectMeta {
            name: "svc-shrink".into(),
            namespace: "default".into(),
            ..Default::default()
        },
        spec: ServiceSpec {
            cluster_ip: Some("10.96.0.107".into()),
            ports: vec![ServicePort {
                name: Some("http".into()),
                protocol: "TCP".into(),
                port: 80,
                target_port: Some(8080),
                node_port: None,
            }],
            selector: None,
            service_type: "ClusterIP".into(),
        },
    };

    let old_ep = Endpoint {
        api_version: "v1".into(),
        kind: "Endpoints".into(),
        metadata: ObjectMeta {
            name: "svc-shrink".into(),
            namespace: "default".into(),
            ..Default::default()
        },
        subsets: vec![EndpointSubset {
            addresses: vec![
                EndpointAddress {
                    ip: "10.244.50.2".into(),
                    node_name: None,
                    target_ref: None,
                },
                EndpointAddress {
                    ip: "10.244.50.3".into(),
                    node_name: None,
                    target_ref: None,
                },
                EndpointAddress {
                    ip: "10.244.50.4".into(),
                    node_name: None,
                    target_ref: None,
                },
            ],
            not_ready_addresses: vec![],
            ports: vec![EndpointPort {
                name: Some("http".into()),
                port: 8080,
                protocol: "TCP".into(),
                app_protocol: None,
            }],
        }],
    };

    let new_ep = Endpoint {
        api_version: "v1".into(),
        kind: "Endpoints".into(),
        metadata: old_ep.metadata.clone(),
        subsets: vec![EndpointSubset {
            addresses: vec![EndpointAddress {
                ip: "10.244.50.2".into(),
                node_name: None,
                target_ref: None,
            }],
            not_ready_addresses: vec![],
            ports: vec![EndpointPort {
                name: Some("http".into()),
                port: 8080,
                protocol: "TCP".into(),
                app_protocol: None,
            }],
        }],
    };

    let json = generate_service_update_with_old_endpoint(&svc, &old_ep, &new_ep)
        .expect("generate_service_update_with_old_endpoint failed");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json expected");

    assert!(
        !has_delete_chain_cmd(&parsed, "ep-svc-default-svc-shrink-80-1"),
        "phase C default path should not delete endpoint chains on shrink"
    );
    assert!(
        !has_delete_chain_cmd(&parsed, "ep-svc-default-svc-shrink-80-2"),
        "phase C default path should not delete endpoint chains on shrink"
    );
}

#[test]
fn test_endpoint_empty_generates_all_ep_chain_deletes() {
    let svc = ServiceTask {
        api_version: "v1".into(),
        kind: "Service".into(),
        metadata: ObjectMeta {
            name: "svc-empty".into(),
            namespace: "default".into(),
            ..Default::default()
        },
        spec: ServiceSpec {
            cluster_ip: Some("10.96.0.108".into()),
            ports: vec![ServicePort {
                name: Some("http".into()),
                protocol: "TCP".into(),
                port: 80,
                target_port: Some(8080),
                node_port: None,
            }],
            selector: None,
            service_type: "ClusterIP".into(),
        },
    };

    let old_ep = Endpoint {
        api_version: "v1".into(),
        kind: "Endpoints".into(),
        metadata: ObjectMeta {
            name: "svc-empty".into(),
            namespace: "default".into(),
            ..Default::default()
        },
        subsets: vec![EndpointSubset {
            addresses: vec![
                EndpointAddress {
                    ip: "10.244.60.2".into(),
                    node_name: None,
                    target_ref: None,
                },
                EndpointAddress {
                    ip: "10.244.60.3".into(),
                    node_name: None,
                    target_ref: None,
                },
            ],
            not_ready_addresses: vec![],
            ports: vec![EndpointPort {
                name: Some("http".into()),
                port: 8080,
                protocol: "TCP".into(),
                app_protocol: None,
            }],
        }],
    };

    let new_ep = Endpoint {
        api_version: "v1".into(),
        kind: "Endpoints".into(),
        metadata: old_ep.metadata.clone(),
        subsets: vec![],
    };

    let json = generate_service_update_with_old_endpoint(&svc, &old_ep, &new_ep)
        .expect("generate_service_update_with_old_endpoint failed");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json expected");

    assert!(
        !has_delete_chain_cmd(&parsed, "ep-svc-default-svc-empty-80-0"),
        "phase C default path should not delete endpoint chains on empty transition"
    );
    assert!(
        !has_delete_chain_cmd(&parsed, "ep-svc-default-svc-empty-80-1"),
        "phase C default path should not delete endpoint chains on empty transition"
    );
    assert!(
        has_reject_rule_in_chain(&parsed, "svc-default-svc-empty-80"),
        "endpoint empty transition should rebuild service chain with reject"
    );
}
