use nftables::batch::Batch;
use nftables::expr::{Expression, NamedExpression, Payload, PayloadField, Verdict};
use nftables::helper;
use nftables::schema::{self, Element, NfListObject, Rule};
use nftables::stmt::{JumpTarget, Statement, VerdictMap};
use nftables::types::{NfChainPolicy, NfChainType, NfFamily, NfHook};
use serde_json::json;
use std::borrow::Cow;
use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn test_hybrid_verdict_map() {
    let uid = nix::unistd::getuid();
    if !uid.is_root() {
        println!("Skipping hybrid test since not root");
        return;
    }

    let table_name = "test_hybrid";

    // 1. First Pass: Raw JSON to create Table and Map with `verdict` type
    let init_payload = json!({
        "nftables": [
            { "metainfo": { "json_schema_version": 1 } },
            { "table": { "family": "ip", "name": table_name } },
            { "map": {
                "family": "ip",
                "table": table_name,
                "name": "cluster_ips",
                "type": ["ipv4_addr", "inet_service"],
                "map": "verdict"
            }}
        ]
    });

    let json_str = init_payload.to_string();
    let mut child = Command::new("nft")
        .args(["-j", "-f", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to spawn nft");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(json_str.as_bytes())
        .unwrap();
    let status = child.wait().unwrap();
    assert!(status.success(), "Failed to apply raw JSON");

    // 2. Second Pass: Add chains, rules, and elements using Batch
    let mut batch = Batch::new();

    let target_chain = "svc-demo";
    batch.add(NfListObject::Chain(schema::Chain {
        family: NfFamily::IP,
        table: Cow::Borrowed(table_name),
        name: Cow::Borrowed(target_chain),
        ..Default::default()
    }));

    let svc_chain = "test-services";
    batch.add(NfListObject::Chain(schema::Chain {
        family: NfFamily::IP,
        table: Cow::Borrowed(table_name),
        name: Cow::Borrowed(svc_chain),
        _type: Some(NfChainType::NAT),
        hook: Some(NfHook::Prerouting),
        prio: Some(0),
        policy: Some(NfChainPolicy::Accept),
        ..Default::default()
    }));

    batch.add(NfListObject::Rule(Rule {
        family: NfFamily::IP,
        table: Cow::Borrowed(table_name),
        chain: Cow::Borrowed(svc_chain),
        expr: Cow::Owned(vec![Statement::VerdictMap(VerdictMap {
            key: Expression::Named(NamedExpression::Concat(vec![
                Expression::Named(NamedExpression::Payload(Payload::PayloadField(
                    PayloadField {
                        protocol: Cow::Borrowed("ip"),
                        field: Cow::Borrowed("daddr"),
                    },
                ))),
                Expression::Named(NamedExpression::Payload(Payload::PayloadField(
                    PayloadField {
                        protocol: Cow::Borrowed("tcp"),
                        field: Cow::Borrowed("dport"),
                    },
                ))),
            ])),
            data: Expression::String(Cow::Borrowed("@cluster_ips")),
        })]),
        ..Default::default()
    }));

    // Add elements to the map
    batch.add(NfListObject::Element(Element {
        family: NfFamily::IP,
        table: Cow::Borrowed(table_name),
        name: Cow::Borrowed("cluster_ips"),
        elem: Cow::Owned(vec![Expression::List(vec![
            Expression::Named(NamedExpression::Concat(vec![
                Expression::String(Cow::Borrowed("10.96.0.1")),
                Expression::Number(80),
            ])),
            Expression::Verdict(Verdict::Goto(JumpTarget {
                target: Cow::Borrowed(target_chain),
            })),
        ])]),
    }));

    let ruleset = batch.to_nftables();
    println!(
        "Generated Batch payload: {}",
        serde_json::to_string_pretty(&ruleset).unwrap()
    );

    match helper::apply_ruleset(&ruleset) {
        Ok(_) => {
            println!("✓ Successfully applied batch update to nftables!");
            let output = Command::new("nft")
                .args(["list", "table", "ip", table_name])
                .output()
                .expect("Failed to list table");
            println!(
                "Table contents:\n{}",
                String::from_utf8_lossy(&output.stdout)
            );
        }
        Err(e) => {
            panic!("apply_ruleset failed: {:?}", e);
        }
    }
}
