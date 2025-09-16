// SPDX-License-Identifier: MIT

use anyhow::Context;
use netlink_packet_utils::{
    traits::{Emitable, Parseable, ParseableParametrized},
    DecodeError,
};

use super::{
    super::AddressFamily, attribute::RTA_ENCAP_TYPE, RouteAttribute,
    RouteHeader, RouteLwEnCapType, RouteMessageBuffer, RouteType,
};

#[derive(Debug, PartialEq, Eq, Clone, Default)]
#[non_exhaustive]
pub struct RouteMessage {
    pub header: RouteHeader,
    pub attributes: Vec<RouteAttribute>,
}

impl Emitable for RouteMessage {
    fn buffer_len(&self) -> usize {
        self.header.buffer_len() + self.attributes.as_slice().buffer_len()
    }

    fn emit(&self, buffer: &mut [u8]) {
        self.header.emit(buffer);
        self.attributes
            .as_slice()
            .emit(&mut buffer[self.header.buffer_len()..]);
    }
}

impl<'a, T: AsRef<[u8]> + 'a> Parseable<RouteMessageBuffer<&'a T>>
    for RouteMessage
{
    fn parse(buf: &RouteMessageBuffer<&'a T>) -> Result<Self, DecodeError> {
        let header = RouteHeader::parse(buf)
            .context("failed to parse route message header")?;
        let address_family = header.address_family;
        let route_type = header.kind;
        Ok(RouteMessage {
            header,
            attributes: Vec::<RouteAttribute>::parse_with_param(
                buf,
                (address_family, route_type),
            )
            .context("failed to parse route message NLAs")?,
        })
    }
}

impl<'a, T: AsRef<[u8]> + 'a>
    ParseableParametrized<RouteMessageBuffer<&'a T>, (AddressFamily, RouteType)>
    for Vec<RouteAttribute>
{
    fn parse_with_param(
        buf: &RouteMessageBuffer<&'a T>,
        (address_family, route_type): (AddressFamily, RouteType),
    ) -> Result<Self, DecodeError> {
        let mut attributes = vec![];
        let mut encap_type = RouteLwEnCapType::None;
        // The RTA_ENCAP_TYPE is provided __after__ RTA_ENCAP, we should find
        // RTA_ENCAP_TYPE first.
        for nla_buf in buf.attributes() {
            let nla = match nla_buf {
                Ok(n) => n,
                Err(_) => continue,
            };
            if nla.kind() == RTA_ENCAP_TYPE {
                if let Ok(RouteAttribute::EncapType(v)) =
                    RouteAttribute::parse_with_param(
                        &nla,
                        (address_family, route_type, encap_type),
                    )
                {
                    encap_type = v;
                    break;
                }
            }
        }
        for nla_buf in buf.attributes() {
            attributes.push(RouteAttribute::parse_with_param(
                &nla_buf?,
                (address_family, route_type, encap_type),
            )?);
        }
        Ok(attributes)
    }
}
