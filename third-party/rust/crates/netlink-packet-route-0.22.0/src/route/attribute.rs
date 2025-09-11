// SPDX-License-Identifier: MIT

use anyhow::Context;
use byteorder::{ByteOrder, NativeEndian};
use netlink_packet_utils::{
    nla::{DefaultNla, Nla, NlaBuffer},
    parsers::{parse_u16, parse_u32, parse_u64, parse_u8},
    traits::{Emitable, Parseable, ParseableParametrized},
    DecodeError,
};

use super::{
    super::AddressFamily, lwtunnel::VecRouteLwTunnelEncap,
    metrics::VecRouteMetric, mpls::VecMplsLabel, MplsLabel, RouteAddress,
    RouteCacheInfo, RouteCacheInfoBuffer, RouteLwEnCapType, RouteLwTunnelEncap,
    RouteMetric, RouteMfcStats, RouteMfcStatsBuffer, RouteMplsTtlPropagation,
    RouteNextHop, RouteNextHopBuffer, RoutePreference, RouteRealm, RouteType,
    RouteVia, RouteViaBuffer,
};

const RTA_DST: u16 = 1;
const RTA_SRC: u16 = 2;
const RTA_IIF: u16 = 3;
const RTA_OIF: u16 = 4;
const RTA_GATEWAY: u16 = 5;
const RTA_PRIORITY: u16 = 6;
const RTA_PREFSRC: u16 = 7;
const RTA_METRICS: u16 = 8;
const RTA_MULTIPATH: u16 = 9;
// const RTA_PROTOINFO: u16 = 10; // linux kernel said `no longer used`
const RTA_FLOW: u16 = 11;
const RTA_CACHEINFO: u16 = 12;
// const RTA_SESSION: u16 = 13; // linux kernel said `no longer used`
// const RTA_MP_ALGO: u16 = 14; // linux kernel said `no longer used`
const RTA_TABLE: u16 = 15;
const RTA_MARK: u16 = 16;
const RTA_MFC_STATS: u16 = 17;
const RTA_VIA: u16 = 18;
const RTA_NEWDST: u16 = 19;
const RTA_PREF: u16 = 20;
pub(crate) const RTA_ENCAP_TYPE: u16 = 21;
const RTA_ENCAP: u16 = 22;
const RTA_EXPIRES: u16 = 23;
const RTA_UID: u16 = 25;
const RTA_TTL_PROPAGATE: u16 = 26;
// TODO
// const RTA_IP_PROTO:u16 = 27;
// const RTA_SPORT:u16 = 28;
// const RTA_DPORT:u16 = 29;
// const RTA_NH_ID:u16 = 30;

/// Netlink attributes for `RTM_NEWROUTE`, `RTM_DELROUTE`,
/// `RTM_GETROUTE` netlink messages.
#[derive(Debug, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum RouteAttribute {
    Metrics(Vec<RouteMetric>),
    MfcStats(RouteMfcStats),
    MultiPath(Vec<RouteNextHop>),
    CacheInfo(RouteCacheInfo),
    Destination(RouteAddress),
    Source(RouteAddress),
    Gateway(RouteAddress),
    PrefSource(RouteAddress),
    Via(RouteVia),
    /// Only for MPLS for destination label(u32) to forward the packet with
    NewDestination(Vec<MplsLabel>),
    Preference(RoutePreference),
    EncapType(RouteLwEnCapType),
    Encap(Vec<RouteLwTunnelEncap>),
    // The RTA_EXPIRES holds different data type in kernel 6.5.8.
    // For non-multipath route, it is u32 and only used for modifying routes.
    // For multipath route, it is u64 for querying only.
    /// This is only for non-multicast route
    Expires(u32),
    /// This is only for multicast route
    MulticastExpires(u64),
    Uid(u32),
    TtlPropagate(RouteMplsTtlPropagation),
    Iif(u32),
    Oif(u32),
    Priority(u32),
    /// IPv4 Realm
    Realm(RouteRealm),
    Table(u32),
    Mark(u32),
    Other(DefaultNla),
}

impl Nla for RouteAttribute {
    fn value_len(&self) -> usize {
        match self {
            Self::Destination(addr)
            | Self::PrefSource(addr)
            | Self::Gateway(addr)
            | Self::Source(addr) => addr.buffer_len(),
            Self::Via(v) => v.buffer_len(),
            Self::NewDestination(v) => VecMplsLabel(v.clone()).buffer_len(),
            Self::Encap(v) => v.as_slice().buffer_len(),
            Self::TtlPropagate(_) => 1,
            Self::CacheInfo(cache_info) => cache_info.buffer_len(),
            Self::MfcStats(stats) => stats.buffer_len(),
            Self::Metrics(metrics) => metrics.as_slice().buffer_len(),
            Self::MultiPath(next_hops) => {
                next_hops.iter().map(|nh| nh.buffer_len()).sum()
            }
            Self::Preference(_) => 1,
            Self::EncapType(v) => v.buffer_len(),
            Self::Realm(v) => v.buffer_len(),
            Self::Uid(_)
            | Self::Expires(_)
            | Self::Iif(_)
            | Self::Oif(_)
            | Self::Priority(_)
            | Self::Table(_)
            | Self::Mark(_) => 4,
            Self::MulticastExpires(_) => 8,
            Self::Other(attr) => attr.value_len(),
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        match self {
            Self::Destination(addr)
            | Self::PrefSource(addr)
            | Self::Source(addr)
            | Self::Gateway(addr) => addr.emit(buffer),
            Self::Via(v) => v.emit(buffer),
            Self::NewDestination(v) => VecMplsLabel(v.to_vec()).emit(buffer),

            Self::Encap(nlas) => nlas.as_slice().emit(buffer),
            Self::TtlPropagate(v) => buffer[0] = u8::from(*v),
            Self::Preference(p) => buffer[0] = (*p).into(),
            Self::CacheInfo(cache_info) => cache_info.emit(buffer),
            Self::MfcStats(stats) => stats.emit(buffer),
            Self::Metrics(metrics) => metrics.as_slice().emit(buffer),
            Self::MultiPath(next_hops) => {
                let mut offset = 0;
                for nh in next_hops {
                    let len = nh.buffer_len();
                    nh.emit(&mut buffer[offset..offset + len]);
                    offset += len
                }
            }
            Self::EncapType(v) => v.emit(buffer),
            Self::Uid(value)
            | Self::Expires(value)
            | Self::Iif(value)
            | Self::Oif(value)
            | Self::Priority(value)
            | Self::Table(value)
            | Self::Mark(value) => NativeEndian::write_u32(buffer, *value),
            Self::Realm(v) => v.emit(buffer),
            Self::MulticastExpires(value) => {
                NativeEndian::write_u64(buffer, *value)
            }
            Self::Other(attr) => attr.emit_value(buffer),
        }
    }

    fn kind(&self) -> u16 {
        match self {
            Self::Destination(_) => RTA_DST,
            Self::Source(_) => RTA_SRC,
            Self::Iif(_) => RTA_IIF,
            Self::Oif(_) => RTA_OIF,
            Self::Gateway(_) => RTA_GATEWAY,
            Self::Priority(_) => RTA_PRIORITY,
            Self::PrefSource(_) => RTA_PREFSRC,
            Self::Metrics(_) => RTA_METRICS,
            Self::MultiPath(_) => RTA_MULTIPATH,
            Self::Realm(_) => RTA_FLOW,
            Self::CacheInfo(_) => RTA_CACHEINFO,
            Self::Table(_) => RTA_TABLE,
            Self::Mark(_) => RTA_MARK,
            Self::MfcStats(_) => RTA_MFC_STATS,
            Self::Via(_) => RTA_VIA,
            Self::NewDestination(_) => RTA_NEWDST,
            Self::Preference(_) => RTA_PREF,
            Self::EncapType(_) => RTA_ENCAP_TYPE,
            Self::Encap(_) => RTA_ENCAP,
            Self::Expires(_) => RTA_EXPIRES,
            Self::MulticastExpires(_) => RTA_EXPIRES,
            Self::Uid(_) => RTA_UID,
            Self::TtlPropagate(_) => RTA_TTL_PROPAGATE,
            Self::Other(ref attr) => attr.kind(),
        }
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized>
    ParseableParametrized<
        NlaBuffer<&'a T>,
        (AddressFamily, RouteType, RouteLwEnCapType),
    > for RouteAttribute
{
    fn parse_with_param(
        buf: &NlaBuffer<&'a T>,
        (address_family, route_type, encap_type): (
            AddressFamily,
            RouteType,
            RouteLwEnCapType,
        ),
    ) -> Result<Self, DecodeError> {
        let payload = buf.value();
        Ok(match buf.kind() {
            RTA_DST => {
                Self::Destination(RouteAddress::parse(address_family, payload)?)
            }
            RTA_SRC => {
                Self::Source(RouteAddress::parse(address_family, payload)?)
            }
            RTA_GATEWAY => {
                Self::Gateway(RouteAddress::parse(address_family, payload)?)
            }
            RTA_PREFSRC => {
                Self::PrefSource(RouteAddress::parse(address_family, payload)?)
            }
            RTA_VIA => Self::Via(
                RouteVia::parse(
                    &RouteViaBuffer::new_checked(payload).context(format!(
                        "Invalid RTA_VIA value {:?}",
                        payload
                    ))?,
                )
                .context(format!("Invalid RTA_VIA value {:?}", payload))?,
            ),
            RTA_NEWDST => Self::NewDestination(
                VecMplsLabel::parse(payload)
                    .context(format!("Invalid RTA_NEWDST value {:?}", payload))?
                    .0,
            ),

            RTA_PREF => Self::Preference(parse_u8(payload)?.into()),
            RTA_ENCAP => Self::Encap(
                VecRouteLwTunnelEncap::parse_with_param(buf, encap_type)?.0,
            ),
            RTA_EXPIRES => {
                if route_type == RouteType::Multicast {
                    Self::MulticastExpires(parse_u64(payload).context(
                        format!(
                            "invalid RTA_EXPIRES (multicast) value {:?}",
                            payload
                        ),
                    )?)
                } else {
                    Self::Expires(parse_u32(payload).context(format!(
                        "invalid RTA_EXPIRES value {:?}",
                        payload
                    ))?)
                }
            }
            RTA_UID => Self::Uid(
                parse_u32(payload)
                    .context(format!("invalid RTA_UID value {:?}", payload))?,
            ),
            RTA_TTL_PROPAGATE => Self::TtlPropagate(
                RouteMplsTtlPropagation::from(parse_u8(payload).context(
                    format!("invalid RTA_TTL_PROPAGATE {:?}", payload),
                )?),
            ),
            RTA_ENCAP_TYPE => Self::EncapType(RouteLwEnCapType::from(
                parse_u16(payload).context("invalid RTA_ENCAP_TYPE value")?,
            )),
            RTA_IIF => {
                Self::Iif(parse_u32(payload).context("invalid RTA_IIF value")?)
            }
            RTA_OIF => {
                Self::Oif(parse_u32(payload).context("invalid RTA_OIF value")?)
            }
            RTA_PRIORITY => Self::Priority(
                parse_u32(payload).context("invalid RTA_PRIORITY value")?,
            ),
            RTA_FLOW => Self::Realm(
                RouteRealm::parse(payload).context("invalid RTA_FLOW value")?,
            ),
            RTA_TABLE => Self::Table(
                parse_u32(payload).context("invalid RTA_TABLE value")?,
            ),
            RTA_MARK => Self::Mark(
                parse_u32(payload).context("invalid RTA_MARK value")?,
            ),

            RTA_CACHEINFO => Self::CacheInfo(
                RouteCacheInfo::parse(
                    &RouteCacheInfoBuffer::new_checked(payload)
                        .context("invalid RTA_CACHEINFO value")?,
                )
                .context("invalid RTA_CACHEINFO value")?,
            ),
            RTA_MFC_STATS => Self::MfcStats(
                RouteMfcStats::parse(
                    &RouteMfcStatsBuffer::new_checked(payload)
                        .context("invalid RTA_MFC_STATS value")?,
                )
                .context("invalid RTA_MFC_STATS value")?,
            ),
            RTA_METRICS => Self::Metrics(
                VecRouteMetric::parse(payload)
                    .context("invalid RTA_METRICS value")?
                    .0,
            ),
            RTA_MULTIPATH => {
                let mut next_hops = vec![];
                let mut buf = payload;
                loop {
                    let nh_buf = RouteNextHopBuffer::new_checked(&buf)
                        .context("invalid RTA_MULTIPATH value")?;
                    let len = nh_buf.length() as usize;
                    let nh = RouteNextHop::parse_with_param(
                        &nh_buf,
                        (address_family, route_type, encap_type),
                    )
                    .context("invalid RTA_MULTIPATH value")?;
                    next_hops.push(nh);
                    if buf.len() == len {
                        break;
                    }
                    buf = &buf[len..];
                }
                Self::MultiPath(next_hops)
            }
            _ => Self::Other(
                DefaultNla::parse(buf).context("invalid NLA (unknown kind)")?,
            ),
        })
    }
}
