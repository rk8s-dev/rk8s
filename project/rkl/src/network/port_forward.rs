use anyhow::Result;
use libruntime::cri::cri_api::PortMapping;
use nftables::{
    expr::{self, Expression, NamedExpression, Payload, PayloadField, Verdict},
    schema::{self, NfCmd, NfListObject, Nftables, Rule},
    stmt::{self, JumpTarget, Statement, VerdictMap},
    types,
};
use std::sync::atomic::AtomicUsize;
use std::{borrow::Cow, collections::HashSet};

const TABLE_NAME: &str = "rk8s";
const CHAIN_DNAT: &str = "dnat";
const MAP_HOST_PORTS: &str = "host_ports";

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn create_unique_chain_name() -> String {
    let count = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("dnat_{}", count.to_string())
}

fn l4proto_number(protocol: i32) -> u32 {
    match protocol {
        1 => 17,
        _ => 6,
    }
}

fn add_elements_to_map<'a, 'b>(
    port_mapping_and_chain_bindings: &'a [(&PortMapping, String)],
    objects: &'b mut Vec<schema::NfObject<'a>>,
) {
    for (port_mapping, chain) in port_mapping_and_chain_bindings {
        objects.push(schema::NfObject::CmdObject(NfCmd::Add(
            schema::NfListObject::Element(schema::Element {
                family: types::NfFamily::IP,
                table: Cow::Borrowed(TABLE_NAME),
                name: Cow::Borrowed(MAP_HOST_PORTS),
                elem: Cow::Owned(vec![Expression::List(vec![
                    Expression::Named(NamedExpression::Concat(vec![
                        Expression::Number(l4proto_number(port_mapping.protocol)),
                        Expression::String(Cow::Borrowed(port_mapping.host_ip.as_str())),
                        Expression::Number(port_mapping.host_port as _),
                    ])),
                    Expression::Verdict(Verdict::Goto(JumpTarget {
                        target: Cow::Borrowed(chain),
                    })),
                ])]),
            }),
        )));
    }
}

fn add_chain_for_each_port_mapping<'a, 'b>(
    port_mapping_and_chain_bindings: &'a [(&'a PortMapping, String)],
    container_ip: &'a str,
    objects: &'b mut Vec<schema::NfObject<'a>>,
) {
    for (port_mapping, chain) in port_mapping_and_chain_bindings {
        objects.push(schema::NfObject::CmdObject(NfCmd::Add(
            schema::NfListObject::Chain(schema::Chain {
                family: types::NfFamily::IP,
                table: Cow::Borrowed(TABLE_NAME),
                name: Cow::Borrowed(chain),
                ..Default::default()
            }),
        )));

        objects.push(schema::NfObject::ListObject(schema::NfListObject::Rule(
            schema::Rule {
                family: types::NfFamily::IP,
                table: Cow::Borrowed(TABLE_NAME),
                chain: Cow::Borrowed(chain),
                expr: Cow::Owned(vec![stmt::Statement::DNAT(Some(stmt::NAT {
                    addr: Some(expr::Expression::String(Cow::Borrowed(container_ip))),
                    family: Some(stmt::NATFamily::IP),
                    port: Some(expr::Expression::Number(port_mapping.container_port as _)),
                    flags: None,
                }))]),
                ..Default::default()
            },
        )));
    }
}

pub fn apply_port_mappings(port_mappings: &[PortMapping], container_ip: &str) -> Result<()> {
    let mut objects: Vec<schema::NfObject> = Vec::new();

    // detects collisions in port mapping and returns an error
    let mut seen = HashSet::new();
    for port_mapping in port_mappings {
        let key = (port_mapping.host_ip.as_str(), port_mapping.host_port);
        if !seen.insert(key) {
            anyhow::bail!(
                "Found conflicting port mapping: {}:{}",
                port_mapping.host_ip,
                port_mapping.host_port
            );
        }
    }

    let port_mapping_and_chain_bindings = port_mappings
        .iter()
        .map(|mapping| (mapping, create_unique_chain_name()))
        .collect::<Vec<_>>();

    objects.push(schema::NfObject::CmdObject(NfCmd::Add(
        schema::NfListObject::Chain(schema::Chain {
            family: types::NfFamily::IP,
            table: Cow::Borrowed(TABLE_NAME),
            name: Cow::Borrowed(CHAIN_DNAT),
            _type: Some(types::NfChainType::NAT),
            hook: Some(types::NfHook::Prerouting),
            prio: Some(-100),
            ..Default::default()
        }),
    )));

    objects.push(schema::NfObject::CmdObject(NfCmd::Add(NfListObject::Rule(
        Rule {
            family: types::NfFamily::IP,
            table: Cow::Borrowed(TABLE_NAME),
            chain: Cow::Borrowed(CHAIN_DNAT),
            expr: Cow::Owned(vec![Statement::VerdictMap(VerdictMap {
                key: Expression::Named(NamedExpression::Concat(vec![
                    Expression::Named(NamedExpression::Payload(Payload::PayloadField(
                        PayloadField {
                            protocol: Cow::Borrowed("ip"),
                            field: Cow::Borrowed("protocol"),
                        },
                    ))),
                    Expression::Named(NamedExpression::Payload(Payload::PayloadField(
                        PayloadField {
                            protocol: Cow::Borrowed("ip"),
                            field: Cow::Borrowed("daddr"),
                        },
                    ))),
                    Expression::Named(NamedExpression::Payload(Payload::PayloadField(
                        PayloadField {
                            protocol: Cow::Borrowed("th"),
                            field: Cow::Borrowed("dport"),
                        },
                    ))),
                ])),
                data: Expression::String(Cow::Borrowed("@host_ports")),
            })]),
            ..Default::default()
        },
    ))));

    add_elements_to_map(port_mapping_and_chain_bindings.as_slice(), &mut objects);
    add_chain_for_each_port_mapping(&port_mapping_and_chain_bindings, container_ip, &mut objects);

    let nftables = Nftables {
        objects: Cow::Borrowed(&objects),
    };

    nftables::helper::apply_ruleset(&nftables)?;

    Ok(())
}

pub fn remove_port_mappings() -> Result<()> {
    let mut objects: Vec<schema::NfObject> = Vec::new();

    objects.push(schema::NfObject::CmdObject(schema::NfCmd::Delete(
        schema::NfListObject::Chain(schema::Chain {
            family: types::NfFamily::IP,
            table: Cow::Borrowed(TABLE_NAME),
            name: Cow::Borrowed(CHAIN_DNAT),
            ..Default::default()
        }),
    )));

    for chain_number in 0..COUNTER.load(std::sync::atomic::Ordering::Relaxed) {
        objects.push(schema::NfObject::CmdObject(schema::NfCmd::Delete(
            schema::NfListObject::Chain(schema::Chain {
                family: types::NfFamily::IP,
                table: Cow::Borrowed(TABLE_NAME),
                name: Cow::Owned(format!("dnat_{chain_number}")),
                ..Default::default()
            }),
        )));
    }

    objects.push(schema::NfObject::CmdObject(schema::NfCmd::Delete(
        schema::NfListObject::Map(Box::new(schema::Map {
            family: types::NfFamily::IP,
            table: Cow::Borrowed(TABLE_NAME),
            name: Cow::Borrowed(MAP_HOST_PORTS),
            ..Default::default()
        })),
    )));

    let nftables = Nftables {
        objects: Cow::Borrowed(&objects),
    };

    nftables::helper::apply_ruleset(&nftables)?;

    Ok(())
}
