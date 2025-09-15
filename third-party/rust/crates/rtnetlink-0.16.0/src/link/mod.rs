// SPDX-License-Identifier: MIT

mod add;
mod bond;
mod bond_port;
mod bridge;
mod builder;
mod del;
mod dummy;
mod get;
mod handle;
mod mac_vlan;
mod mac_vtap;
mod property_add;
mod property_del;
mod set;
mod veth;
mod vlan;
mod vrf;
mod vxlan;
mod wireguard;
mod xfrm;

pub use self::add::LinkAddRequest;
pub use self::bond::LinkBond;
pub use self::bond_port::LinkBondPort;
pub use self::bridge::LinkBridge;
pub use self::builder::{LinkMessageBuilder, LinkUnspec};
pub use self::del::LinkDelRequest;
pub use self::dummy::LinkDummy;
pub use self::get::LinkGetRequest;
pub use self::handle::LinkHandle;
pub use self::mac_vlan::LinkMacVlan;
pub use self::mac_vtap::LinkMacVtap;
pub use self::property_add::LinkNewPropRequest;
pub use self::property_del::LinkDelPropRequest;
pub use self::set::LinkSetRequest;
pub use self::veth::LinkVeth;
pub use self::vlan::{LinkVlan, QosMapping};
pub use self::vrf::LinkVrf;
pub use self::vxlan::LinkVxlan;
pub use self::wireguard::LinkWireguard;
pub use self::xfrm::LinkXfrm;

#[cfg(test)]
mod test;
