use anyhow::Result;
use libruntime::cri::cri_api::PortMapping;
use nftables::{
    expr::{self, Expression, Map, NamedExpression, Payload, PayloadField},
    schema::{self, NfCmd, NfListObject, Nftables, Rule},
    stmt::{self, Statement, VerdictMap},
    types,
};
use std::borrow::Cow;

const TABLE_NAME: &str = "rk8s";
const CHAIN_DNAT: &str = "dnat";
const MAP_HOST_PORTS: &str = "host_ports";

fn create_port_mapping_exprs<'a>(
    port_mappings: &'a [PortMapping],
    container_ip: &'a str,
) -> Vec<expr::Expression<'a>> {
    port_mappings
        .iter()
        .map(|mapping| {
            Expression::List(vec![Expression::Named(NamedExpression::Map(Box::new(
                Map {
                    key: Expression::Named(NamedExpression::Concat(vec![
                        Expression::String(Cow::Borrowed(&mapping.host_ip)),
                        Expression::Number(mapping.host_port as _),
                    ])),
                    data: Expression::Named(NamedExpression::Concat(vec![
                        Expression::String(Cow::Borrowed(container_ip)),
                        Expression::Number(mapping.container_port as _),
                    ])),
                },
            )))])
        })
        .collect::<Vec<_>>()
}

pub fn apply_port_mappings(port_mappings: &[PortMapping], container_ip: &str) -> Result<()> {
    let mut objects: Vec<schema::NfObject> = Vec::new();

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

    // this is completely wrong, I was thinking if I could get away without creating a separate chain for 
    // each mapping but I would have to create the chains...working on this...
    objects.push(schema::NfObject::CmdObject(NfCmd::Add(NfListObject::Rule(
        Rule {
            family: types::NfFamily::IP,
            table: Cow::Borrowed(TABLE_NAME),
            chain: Cow::Borrowed(CHAIN_DNAT),
            expr: Cow::Owned(vec![
                stmt::Statement::DNAT(Some(stmt::NAT {
                    addr: Some(Expression::Named(NamedExpression::Payload(
                        Payload::PayloadField(PayloadField {
                            protocol: Cow::Borrowed("ip"),
                            field: Cow::Borrowed("daddr"),
                        }),
                    ))),
                    family: Some(stmt::NATFamily::IP),
                    port: Some(Expression::Named(NamedExpression::Payload(
                        Payload::PayloadField(PayloadField {
                            protocol: Cow::Borrowed("tcp"),
                            field: Cow::Borrowed("dport"),
                        }),
                    ))),
                    flags: None,
                })),
                Statement::VerdictMap(VerdictMap {
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
                    data: Expression::String(Cow::Borrowed("@host_ports")),
                }),
            ]),
            ..Default::default()
        },
    ))));

    let port_mapping_exprs = create_port_mapping_exprs(port_mappings, container_ip);
    // add elements to the map
    objects.push(schema::NfObject::CmdObject(NfCmd::Add(
        schema::NfListObject::Element(schema::Element {
            family: types::NfFamily::IP,
            table: Cow::Borrowed(TABLE_NAME),
            name: Cow::Borrowed(MAP_HOST_PORTS),
            elem: Cow::Borrowed(&port_mapping_exprs),
        }),
    )));

    let nftables = Nftables {
        objects: Cow::Borrowed(&objects),
    };

    nftables::helper::apply_ruleset(&nftables)?;

    Ok(())
}

#[allow(unused)]
pub fn remove_port_mappings(port_mappings: &[PortMapping], container_ip: &str) -> Result<()> {
    todo!();
}
