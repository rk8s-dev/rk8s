// SPDX-License-Identifier: MIT

mod address;
mod attribute;
mod cache_info;
mod flags;
mod header;
mod lwtunnel;
mod message;
pub(crate) mod metrics;
mod mfc_stats;
mod mpls;
mod next_hops;
mod preference;
mod realm;
mod via;

#[cfg(test)]
mod tests;

pub use self::address::RouteAddress;
pub use self::attribute::RouteAttribute;
pub use self::cache_info::{RouteCacheInfo, RouteCacheInfoBuffer};
pub use self::header::{
    RouteHeader, RouteMessageBuffer, RouteProtocol, RouteScope, RouteType,
};
pub use self::lwtunnel::{RouteLwEnCapType, RouteLwTunnelEncap};
pub use self::message::RouteMessage;
pub use self::metrics::RouteMetric;
pub use self::mfc_stats::{RouteMfcStats, RouteMfcStatsBuffer};
pub use self::mpls::{MplsLabel, RouteMplsIpTunnel, RouteMplsTtlPropagation};
pub use self::next_hops::{
    RouteNextHop, RouteNextHopBuffer, RouteNextHopFlags,
};
pub use self::preference::RoutePreference;
pub use self::realm::RouteRealm;
pub use self::via::{RouteVia, RouteViaBuffer};
pub use flags::RouteFlags;
