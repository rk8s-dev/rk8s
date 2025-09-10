// SPDX-License-Identifier: MIT

use std::marker::PhantomData;
use std::os::fd::RawFd;

use crate::packet_route::link::{
    InfoData, InfoKind, InfoPortData, InfoPortKind, LinkAttribute, LinkFlags,
    LinkHeader, LinkInfo, LinkMessage,
};

/// Generic interface without interface type
/// Could be used to match interface by interface name or index.
/// Example on attaching a interface to controller
/// ```no_run
/// use rtnetlink::{new_connection, LinkUnspec};
///
/// #[tokio::main]
/// async fn main() -> Result<(), String> {
///     let (connection, handle, _) = new_connection().unwrap();
///     tokio::spawn(connection);
///
///     let controller_index = 63u32;
///
///     handle
///         .link()
///         .set(
///             LinkUnspec::new_with_name("my-nic")
///                 .controller(controller_index)
///                 .build(),
///         )
///         .execute()
///         .await
///         .map_err(|e| format!("{e}"))
/// }
/// ```
#[derive(Debug)]
pub struct LinkUnspec;

impl LinkUnspec {
    /// Equal to `LinkMessageBuilder::<LinkUnspec>::default().index()`
    pub fn new_with_index(index: u32) -> LinkMessageBuilder<Self> {
        LinkMessageBuilder::<LinkUnspec>::default().index(index)
    }

    /// Equal to `LinkMessageBuilder::<LinkUnspec>::default().name()`
    pub fn new_with_name(name: &str) -> LinkMessageBuilder<Self> {
        LinkMessageBuilder::<LinkUnspec>::default().name(name.to_string())
    }
}

#[derive(Debug)]
/// Helper struct for building [LinkMessage].
/// The [LinkMessageBuilder] is designed for advanced user, wrapper
/// structs/functions are created
pub struct LinkMessageBuilder<T> {
    pub(crate) header: LinkHeader,
    pub(crate) info_kind: Option<InfoKind>,
    pub(crate) info_data: Option<InfoData>,
    pub(crate) port_kind: Option<InfoPortKind>,
    pub(crate) port_data: Option<InfoPortData>,
    extra_attriutes: Vec<LinkAttribute>,
    _phantom: PhantomData<T>,
}

impl<T> Default for LinkMessageBuilder<T> {
    fn default() -> Self {
        Self {
            header: Default::default(),
            info_kind: None,
            info_data: Default::default(),
            extra_attriutes: Default::default(),
            port_kind: None,
            port_data: None,
            _phantom: Default::default(),
        }
    }
}

impl<T> LinkMessageBuilder<T> {
    pub fn new_with_info_kind(info_kind: InfoKind) -> Self {
        Self {
            info_kind: Some(info_kind),
            ..Default::default()
        }
    }

    /// Set arbitrary [LinkHeader]
    pub fn set_header(self, header: LinkHeader) -> Self {
        let mut ret = self;
        ret.header = header;
        ret
    }

    /// Append arbitrary [LinkAttribute]
    pub fn append_extra_attribute(self, link_attr: LinkAttribute) -> Self {
        let mut ret = self;
        ret.extra_attriutes.push(link_attr);
        ret
    }

    /// Set arbitrary [InfoData]
    pub fn set_info_data(self, info_data: InfoData) -> Self {
        let mut ret = self;
        ret.info_data = Some(info_data);
        ret
    }

    /// Set the link up (equivalent to `ip link set dev DEV up`)
    pub fn up(self) -> Self {
        let mut ret = self;
        ret.header.flags |= LinkFlags::Up;
        ret.header.change_mask |= LinkFlags::Up;
        ret
    }

    /// Set the link down (equivalent to `ip link set dev DEV down`)
    pub fn down(self) -> Self {
        let mut ret = self;
        ret.header.flags.remove(LinkFlags::Up);
        ret.header.change_mask |= LinkFlags::Up;
        ret
    }

    /// Enable or disable promiscious mode of the link with the given index
    /// (equivalent to `ip link set dev DEV promisc on/off`)
    pub fn promiscuous(self, enable: bool) -> Self {
        let mut ret = self;
        if enable {
            ret.header.flags |= LinkFlags::Promisc;
        } else {
            ret.header.flags.remove(LinkFlags::Promisc);
        }
        ret.header.change_mask |= LinkFlags::Promisc;
        ret
    }

    /// Enable or disable the ARP protocol of the link with the given index
    /// (equivalent to `ip link set dev DEV arp on/off`)
    pub fn arp(self, enable: bool) -> Self {
        let mut ret = self;
        if enable {
            ret.header.flags.remove(LinkFlags::Noarp);
        } else {
            ret.header.flags |= LinkFlags::Noarp;
        }
        ret.header.change_mask |= LinkFlags::Noarp;
        ret
    }

    pub fn name(self, name: String) -> Self {
        self.append_extra_attribute(LinkAttribute::IfName(name))
    }

    /// Set the mtu of the link with the given index (equivalent to
    /// `ip link set DEV mtu MTU`)
    pub fn mtu(self, mtu: u32) -> Self {
        self.append_extra_attribute(LinkAttribute::Mtu(mtu))
    }

    /// Kernel index number of interface, used for querying, modifying or
    /// deleting existing interface.
    pub fn index(self, index: u32) -> Self {
        let mut ret = self;
        ret.header.index = index;
        ret
    }

    /// Define the hardware address of the link when creating it (equivalent to
    /// `ip link add NAME address ADDRESS`)
    pub fn address(self, address: Vec<u8>) -> Self {
        self.append_extra_attribute(LinkAttribute::Address(address))
    }

    /// Move this network device into the network namespace of the process with
    /// the given `pid`.
    pub fn setns_by_pid(self, pid: u32) -> Self {
        self.append_extra_attribute(LinkAttribute::NetNsPid(pid))
    }

    /// Move this network device into the network namespace corresponding to the
    /// given file descriptor.
    pub fn setns_by_fd(self, fd: RawFd) -> Self {
        self.append_extra_attribute(LinkAttribute::NetNsFd(fd))
    }

    /// The physical device to act operate on. (e.g. the parent interface of
    /// VLAN/VxLAN)
    pub fn link(self, index: u32) -> Self {
        self.append_extra_attribute(LinkAttribute::Link(index))
    }

    /// Define controller interface index (similar to
    /// ip link set NAME master CONTROLLER_NAME)
    pub fn controller(self, ctrl_index: u32) -> Self {
        self.append_extra_attribute(LinkAttribute::Controller(ctrl_index))
    }

    /// Detach the link from its _controller_. This is equivalent to `ip link
    /// set LINK nomaster`. To succeed, the link that is being detached must be
    /// UP.
    pub fn nocontroller(self) -> Self {
        self.append_extra_attribute(LinkAttribute::Controller(0))
    }

    pub fn set_port_kind(self, port_kind: InfoPortKind) -> Self {
        let mut ret = self;
        ret.port_kind = Some(port_kind);
        ret
    }

    /// Include port settings.
    /// The [LinkBondPort] and [LinkBridgePort] are the helper
    pub fn set_port_data(self, port_data: InfoPortData) -> Self {
        let mut ret = self;
        ret.port_data = Some(port_data);
        ret
    }

    pub fn build(self) -> LinkMessage {
        let mut message = LinkMessage::default();
        message.header = self.header;

        if !self.extra_attriutes.is_empty() {
            message.attributes = self.extra_attriutes;
        }

        let mut link_infos: Vec<LinkInfo> = Vec::new();
        if let Some(info_kind) = self.info_kind {
            link_infos.push(LinkInfo::Kind(info_kind));
        }
        if let Some(info_data) = self.info_data {
            link_infos.push(LinkInfo::Data(info_data));
        }

        if let Some(port_kind) = self.port_kind {
            link_infos.push(LinkInfo::PortKind(port_kind));
        }

        if let Some(port_data) = self.port_data {
            link_infos.push(LinkInfo::PortData(port_data));
        }

        if !link_infos.is_empty() {
            message.attributes.push(LinkAttribute::LinkInfo(link_infos));
        }

        message
    }
}

impl LinkMessageBuilder<LinkUnspec> {
    pub fn new() -> Self {
        Self::default()
    }
}
