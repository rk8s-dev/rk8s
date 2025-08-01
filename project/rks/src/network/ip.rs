use std::net::{Ipv4Addr, Ipv6Addr};
use ipnetwork::{Ipv4Network, Ipv6Network};
use num_bigint::BigUint;
use anyhow::{Result, anyhow};
pub trait AddIP<N> {
    fn add(self, n: N) -> Self;
}

pub trait SubIP<N> {
    fn sub(self, n: N) -> Self;
}

impl AddIP<u32> for Ipv4Addr {
    fn add(self, n: u32) -> Self {
        Ipv4Addr::from(u32::from(self) + n)
    }
}

impl SubIP<u32> for Ipv4Addr {
    fn sub(self, n: u32) -> Self {
        Ipv4Addr::from(u32::from(self) - n)
    }
}

impl AddIP<&BigUint> for Ipv6Addr {
    fn add(self, n: &BigUint) -> Self {
        let ip_u = BigUint::from(self.to_bits());
        let sum = ip_u + n;
        let bytes = sum.to_bytes_be();
        let mut padded = [0u8; 16];
        let offset = 16 - bytes.len();
        padded[offset..].copy_from_slice(&bytes);
        Ipv6Addr::from(padded)
    }
}

impl SubIP<&BigUint> for Ipv6Addr {
    fn sub(self, n: &BigUint) -> Self {
        let ip_u = BigUint::from(self.to_bits());
        let diff = ip_u - n;
        let bytes = diff.to_bytes_be();
        let mut padded = [0u8; 16];
        let offset = 16 - bytes.len();
        padded[offset..].copy_from_slice(&bytes);
        Ipv6Addr::from(padded)
    }
}

pub fn next_ipv4_network(net: Ipv4Network) -> Result<Ipv4Network> {
    let next_ip = u32::from(net.ip()).wrapping_add(1 << (32 - net.prefix())) as u32;
    let next_ip = Ipv4Addr::from(next_ip);
    Ipv4Network::new(next_ip, net.prefix()).map_err(|e| anyhow!(e))
}

pub fn next_ipv6_network(net: Ipv6Network) -> Result<Ipv6Network> {

    let bytes = net.ip().octets();
    let increment = 1u128 << (128 - net.prefix());

    let current = u128::from_be_bytes(bytes);
    let next = current.wrapping_add(increment);
    let next_ip = Ipv6Addr::from(next.to_be_bytes());

    Ipv6Network::new(next_ip, net.prefix()).map_err(|e| anyhow!(e))
}
