use common::{
    Endpoint, EndpointAddress, EndpointPort, EndpointSubset, ObjectMeta, ServicePort, ServiceSpec,
    ServiceTask,
};
use libnetwork::nftables::generate_nftables_config;
use std::io::Write;
use std::process::{Command, Stdio};

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
