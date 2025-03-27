use crate::ns::ns::{self, Netns};
use crate::ip::link;
use macaddr::MacAddr6;
use netlink_packet_route::link::{InfoData, InfoKind, InfoVeth};
use rtnetlink::{LinkMessageBuilder, LinkVeth, LinkUnspec};
use cni_plugin::{macaddr::MacAddr, reply};
use anyhow::{anyhow, Error, Result};
use log::info;
use rand::random;

/// Represents a network interface with a name, optional MAC address, and namespace.
#[derive(Debug, Clone)]
pub struct Interface {
    pub name: String,        // Interface name
    pub mac: Option<MacAddr>, // Optional MAC address
    pub ns: Netns,          // Network namespace
}

/// Represents a Veth pair with two interfaces.
#[derive(Debug, Clone)]
pub struct Veth {
    pub interface: Interface, // First interface of the veth pair
    pub peer_inf: Interface,  // Peer interface of the veth pair
}

impl Interface {
    /// Creates a new Interface instance.
    ///
    /// # Arguments
    /// * `name` - The name of the network interface.
    /// * `mac` - Optional MAC address.
    /// * `ns` - The network namespace the interface belongs to.
    ///
    /// # Returns
    /// A new `Interface` instance.
    pub fn new(name: &str, mac: Option<MacAddr>, ns: Netns) -> Self {
        Self {
            name: name.to_string(),
            mac,
            ns,
        }
    }
}

impl Veth {
    /// /// Creates a new Veth pair.
    ///
    /// # Arguments
    /// * `inf_name` - Name of the first interface.
    /// * `peer_name` - Name of the peer interface.
    /// * `ns1` - Namespace for the first interface.
    /// * `ns2` - Namespace for the peer interface.
    ///
    /// # Returns
    /// A new `Veth` instance.
    pub fn new(inf_name: &str, peer_name: &str, ns1: Netns , ns2: Netns) -> Self {
        let interface = Interface::new(inf_name, None, ns1);
        let peer_inf = Interface::new(peer_name, None, ns2);
        
        Self { interface, peer_inf }
    }
     /// Sets the MAC address for the veth interface.
    ///
    /// # Arguments
    /// * `mac` - The MAC address to be assigned.
    ///
    /// # Returns
    /// The updated `Veth` instance with the MAC address set.
    pub fn set_mac_address(mut self, mac: MacAddr) -> Self {
        self.interface.mac = Some(mac);
        self
    }

    /// Sets the MAC address for the peer interface.
    ///
    /// # Arguments
    /// * `mac` - The MAC address to be assigned to the peer interface.
    ///
    /// # Returns
    /// The updated `Veth` instance with the peer MAC address set.
    pub fn set_peer_mac_address(mut self, mac: MacAddr) -> Self {
        self.peer_inf.mac = Some(mac);
        self
    }

    /// Converts `Veth` into a `LinkMessageBuilder` for network configuration.
    ///
    /// # Returns
    /// A configured `LinkMessageBuilder` for veth setup.
    pub fn into_builder(self) -> LinkMessageBuilder<LinkVeth> {
        let build =LinkMessageBuilder::<LinkVeth>::new_with_info_kind(InfoKind::Veth)
            .name(self.interface.name.to_string())
            .setns_by_fd(self.interface.ns.clone().into_fd()).up();

        let peer_msg = LinkMessageBuilder::<LinkUnspec>::new()
            .name(self.peer_inf.name.to_string())
            .setns_by_fd(self.peer_inf.ns.clone().into_fd())
            .build();

        build.set_info_data(InfoData::Veth(InfoVeth::Peer(peer_msg)))
    }

    /// Converts `Veth` into reply interfaces used for CNI responses.
    ///
    /// # Returns
    /// A tuple containing container and host interfaces.
    pub fn to_interface(self) -> anyhow::Result<(reply::Interface,reply::Interface)>{
        let container_inf = reply::Interface{
            name: self.interface.name,
            mac: self.interface.mac,
            sandbox: self.interface.ns.path().unwrap_or(Default::default()),
        };
        let host_inf = reply::Interface{
            name: self.peer_inf.name,
            mac: self.peer_inf.mac,
            sandbox: self.peer_inf.ns.path().unwrap_or(Default::default()),
        };
        Ok((container_inf,host_inf))
    }
}

/// Creates a veth pair and sets it up.
///
/// # Arguments
/// * `container_veth_name` - Name of the container-side veth.
/// * `host_veth_name` - Name of the host-side veth.
/// * `mtu` - MTU size for the veth.
/// * `container_veth_mac` - MAC address for the container-side veth.
/// * `host_ns` - Host network namespace.
/// * `container_ns` - Container network namespace.
///
/// # Returns
/// * `Ok(Veth)` - Successfully created veth pair.
/// * `Err(Error)` - Failed to create veth pair.
pub async fn setup_veth(
    container_veth_name: &str,
    host_veth_name: &str,
    mtu: u32,
    container_veth_mac: &MacAddr,
    host_ns: &Netns,
    container_ns: &Netns,
) -> anyhow::Result<Veth, Error> {  

    let current_ns = Netns::get()?;
    anyhow::ensure!(&current_ns == container_ns, "Network namespace mismatch");

    let (host_inf_name, veth) = make_veth(
        container_veth_name,
        host_veth_name,
        mtu,
        container_veth_mac,
        host_ns,
        container_ns,
    ).await?;

    ns::exec_netns(&current_ns, host_ns, async{
        let link = link::link_by_name(&host_inf_name).await
        .map_err(|e| anyhow!("{}", e))?;
        
        link::link_set_up(&link).await
    }).await?;

    Ok(veth)
}

/// Creates a veth pair with retries if necessary.
///
/// # Arguments
/// * `container_veth_name` - Name of the container-side veth.
/// * `host_veth_name` - Name of the host-side veth.
/// * `mtu` - MTU size for the veth.
/// * `container_veth_mac` - MAC address for the container-side veth.
/// * `host_ns` - Host network namespace.
/// * `container_ns` - Container network namespace.
///
/// # Returns
/// * `Ok((String, Veth))` - Successfully created veth pair, returning the host veth name and the `Veth` instance.
/// * `Err(anyhow::Error)` - Failed to create veth pair after multiple attempts.
async fn make_veth(
    container_veth_name: &str,
    host_veth_name: &str,
    mtu: u32,
    container_veth_mac: &MacAddr,
    host_ns: &Netns,
    container_ns: &Netns,
) -> Result<(String, Veth)> {
    let cur_ns = Netns::get()?;
    anyhow::ensure!(&cur_ns == container_ns, "Current network namespace does not match the target container namespace");

    let mut peer_name = host_veth_name.to_string();
    for attempt in 1..=10 {
        if host_veth_name.is_empty() {
            peer_name = random_veth_name();
        }
        let res = make_veth_pair(
            container_veth_name,
            &peer_name,
            mtu,
            container_veth_mac,
            host_ns,
            container_ns,
        ).await;
        match res {
            Ok(veth) => return Ok((peer_name, veth)),
            Err(e) => {
                log::error!(
                    "Attempt {}/10: Failed to create veth pair - {:?}. Peer: {}, Container: {}",
                    attempt, e, peer_name, container_veth_name
                );
            }
        }
    }
    Err(anyhow!("failed to create veth pair after 10 attempts"))
}

/// Creates a veth pair and configures it.
///
/// # Arguments
/// * `container_veth_name` - Name of the container-side veth.
/// * `host_veth_name` - Name of the host-side veth.
/// * `mtu` - MTU size for the veth.
/// * `container_veth_mac` - MAC address for the container-side veth.
/// * `host_ns` - Host network namespace.
/// * `container_ns` - Container network namespace.
///
/// # Returns
/// * `Ok(Veth)` - Successfully created and configured the veth pair.
/// * `Err(anyhow::Error)` - Failed to create or configure the veth pair.
async fn make_veth_pair(
    container_veth_name: &str,
    host_veth_name: &str,
    mtu: u32,
    container_veth_mac: &MacAddr,
    host_ns: &Netns,
    container_ns: &Netns,
) -> Result<Veth, Error> {
     // Verify that we are in the correct namespace
    let current_ns = Netns::get()?;
    anyhow::ensure!(&current_ns == container_ns, "Current namespace does not match the target container namespace");
    info!(
        "Creating veth pair: container_veth_name={}, host_veth_name={}",
        container_veth_name, host_veth_name
    );
    // Convert MAC address to a format required by the builder
    let container_mac = MacAddr6::from(container_veth_mac.clone()).into_array().to_vec();

    // Initialize the veth pair
    let veth = Veth::new(container_veth_name,host_veth_name,container_ns.clone(),host_ns.clone()).set_mac_address(container_veth_mac.clone());

    // Build veth pair configuration
    let builder = veth.clone().into_builder().mtu(mtu).up().address(container_mac.clone());
    
    link::add_link(builder.build()).await.map_err(|e| anyhow!("Failed to add link: {}", e))?;
    
    Ok(veth)
}

/// Generates a random veth name.
///
/// # Returns
/// * A string representing the randomly generated veth name
fn random_veth_name() -> String {
    let entropy: [u8; 4] = random();
    format!(
        "veth{:02x}{:02x}{:02x}{:02x}",
        entropy[0], entropy[1], entropy[2], entropy[3]
    )
}