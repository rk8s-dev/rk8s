// SPDX-License-Identifier: MIT

use std::{env, error::Error as StdError, str::FromStr};

use rtnetlink::{new_connection, LinkVlan, QosMapping};

fn parse_mapping(parameter: &str) -> Result<QosMapping, Box<dyn StdError>> {
    let (from, to) = parameter
        .split_once(':')
        .ok_or("Failed to parse mapping..")?;

    Ok(QosMapping {
        from: u32::from_str(from)?,
        to: u32::from_str(to)?,
    })
}

const ARG_BASE: &str = "--base";
const ARG_NAME: &str = "--name";
const ARG_ID: &str = "--id";
const ARG_INGRESS_QOS: &str = "--ingress-qos-mapping";
const ARG_EGRESS_QOS: &str = "--egress-qos-mapping";

enum ParsingMode {
    None,
    Base,
    Name,
    Id,
    Ingress,
    Egress,
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let mut args: Vec<String> = env::args().collect();

    let mut base_interface = None;
    let mut name = None;
    let mut id = None;
    let mut ingress = Vec::new();
    let mut egress = Vec::new();

    let mut mode = ParsingMode::None;
    for argument in args.drain(1..) {
        fn match_argument(argument: String) -> Result<ParsingMode, String> {
            match argument.to_lowercase().as_str() {
                ARG_BASE => Ok(ParsingMode::Base),
                ARG_NAME => Ok(ParsingMode::Name),
                ARG_ID => Ok(ParsingMode::Id),
                ARG_INGRESS_QOS => Ok(ParsingMode::Ingress),
                ARG_EGRESS_QOS => Ok(ParsingMode::Egress),
                other => {
                    usage();
                    Err(format!("Unexpected argument: {other}"))
                }
            }
        }

        mode = match mode {
            ParsingMode::None => match_argument(argument)?,
            ParsingMode::Base => {
                base_interface = u32::from_str(&argument).ok();
                ParsingMode::None
            }
            ParsingMode::Name => {
                name = Some(argument);
                ParsingMode::None
            }
            ParsingMode::Id => {
                id = u16::from_str(&argument).ok();
                ParsingMode::None
            }
            mode @ ParsingMode::Ingress => match parse_mapping(&argument) {
                Ok(mapping) => {
                    ingress.push(mapping);
                    mode
                }
                Err(_) => match_argument(argument)?,
            },
            mode @ ParsingMode::Egress => match parse_mapping(&argument) {
                Ok(mapping) => {
                    egress.push(mapping);
                    mode
                }
                Err(_) => match_argument(argument)?,
            },
        }
    }

    let Some(base) = base_interface else {
        usage();
        return Err(
            "Missing or invalid argument for base interface!".to_owned()
        );
    };

    let Some(name) = name else {
        usage();
        return Err(
            "Missing or invalid argument for new interface name!".to_owned()
        );
    };

    let Some(id) = id else {
        usage();
        return Err("Missing or invalid argument for vlan id!".to_owned());
    };

    let (connection, handle, _) = new_connection().unwrap();
    tokio::spawn(connection);

    let message = LinkVlan::new(&name, base, id)
        .up()
        .qos(ingress, egress)
        .build();

    handle
        .link()
        .add(message)
        .execute()
        .await
        .map_err(|err| format!("Netlink request failed: {err}"))
}

fn usage() {
    eprintln!(
        "usage:
    cargo run --example create_vlan -- --base <base link index> --name <link name> --id <vlan id> [--ingress-qos-mapping <mapping as <integer>:<integer> ..>] [--egress-qos-mapping <mapping as <integer>:<integer> ..>]

Note that you need to run this program as root. Instead of running cargo as root,
build the example normally:

    cd netlink-ip ; cargo build --example create_vlan

Then find the binary in the target directory:

    cd ../target/debug/example ; sudo ./create_vlan <link_name>"
    );
}
