// SPDX-License-Identifier: MIT

mod bridge;
mod inet;
mod inet6;
mod inet6_cache;
mod inet6_devconf;
mod inet6_icmp;
mod inet6_iface_flag;
mod inet6_stats;
mod unspec;

pub use self::bridge::{
    AfSpecBridge, BridgeFlag, BridgeMode, BridgeVlanInfo, BridgeVlanInfoFlags,
    BridgeVlanTunnelInfo,
};
pub use self::inet::{AfSpecInet, InetDevConf};
pub use self::inet6::AfSpecInet6;
pub use self::inet6_cache::{Inet6CacheInfo, Inet6CacheInfoBuffer};
pub use self::inet6_devconf::{Inet6DevConf, Inet6DevConfBuffer};
pub use self::inet6_icmp::{Icmp6Stats, Icmp6StatsBuffer};
pub use self::inet6_iface_flag::Inet6IfaceFlags;
pub use self::inet6_stats::{Inet6Stats, Inet6StatsBuffer};
pub use self::unspec::AfSpecUnspec;

#[cfg(any(target_os = "linux", target_os = "fuchsia"))]
pub(crate) use self::bridge::VecAfSpecBridge;
pub(crate) use self::inet::VecAfSpecInet;
pub(crate) use self::inet6::VecAfSpecInet6;
pub(crate) use self::unspec::VecAfSpecUnspec;
