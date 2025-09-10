// SPDX-License-Identifier: MIT

const ICMPV6_ROUTER_PREF_LOW: u8 = 0x3;
const ICMPV6_ROUTER_PREF_MEDIUM: u8 = 0x0;
const ICMPV6_ROUTER_PREF_HIGH: u8 = 0x1;
const ICMPV6_ROUTER_PREF_INVALID: u8 = 0x2;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[non_exhaustive]
pub enum RoutePreference {
    Low,
    Medium,
    High,
    Invalid,
    Other(u8),
}

impl From<RoutePreference> for u8 {
    fn from(v: RoutePreference) -> Self {
        match v {
            RoutePreference::Low => ICMPV6_ROUTER_PREF_LOW,
            RoutePreference::Medium => ICMPV6_ROUTER_PREF_MEDIUM,
            RoutePreference::High => ICMPV6_ROUTER_PREF_HIGH,
            RoutePreference::Invalid => ICMPV6_ROUTER_PREF_INVALID,
            RoutePreference::Other(s) => s,
        }
    }
}

impl From<u8> for RoutePreference {
    fn from(d: u8) -> Self {
        match d {
            ICMPV6_ROUTER_PREF_LOW => Self::Low,
            ICMPV6_ROUTER_PREF_MEDIUM => Self::Medium,
            ICMPV6_ROUTER_PREF_HIGH => Self::High,
            ICMPV6_ROUTER_PREF_INVALID => Self::Invalid,
            _ => Self::Other(d),
        }
    }
}

impl Default for RoutePreference {
    fn default() -> Self {
        Self::Invalid
    }
}
