//! This module is a Rust replica of
//! <https://github.com/hashicorp/vault/blob/main/sdk/helper/cidrutil/cidr.go>

use std::{
    collections::HashSet,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    str::FromStr,
};

use ipnetwork::IpNetwork;

use super::sock_addr::{new_sock_addr, SockAddr, SockAddrType};
use crate::errors::RvError;

pub fn is_ip_addr(addr: &dyn SockAddr) -> bool {
    (addr.sock_addr_type() as u8 & SockAddrType::IP as u8) != 0
}

pub fn remote_addr_is_ok(remote_addr: &str, bound_cidrs: &[Box<dyn SockAddr>]) -> bool {
    if bound_cidrs.is_empty() {
        return true;
    }

    if let Ok(addr) = new_sock_addr(remote_addr) {
        for cidr in bound_cidrs.iter() {
            if is_ip_addr(cidr.as_ref()) && cidr.contains(addr.as_ref()) {
                return true;
            }
        }
    }

    false
}

pub fn ip_belongs_to_cidr(ip_addr: &str, cidr: &str) -> Result<bool, RvError> {
    if ip_addr.is_empty() {
        return Err(RvError::ErrResponse("missing IP address".to_string()));
    }

    let ip = IpAddr::from_str(ip_addr)?;
    let ipnet = IpNetwork::from_str(cidr)?;

    Ok(ipnet.contains(ip))
}

pub fn ip_belongs_to_cidrs(ip_addr: &str, cidrs: &[&str]) -> Result<bool, RvError> {
    if ip_addr.is_empty() {
        return Err(RvError::ErrResponse("missing IP address".to_string()));
    }

    if cidrs.is_empty() {
        return Err(RvError::ErrResponse("missing CIDR blocks to be checked against".to_string()));
    }

    for cidr in cidrs.iter() {
        if ip_belongs_to_cidr(ip_addr, cidr)? {
            return Ok(true);
        }
    }

    Ok(false)
}

pub fn validate_cidr_string(cidr_list: &str, separator: &str) -> Result<bool, RvError> {
    if cidr_list.is_empty() {
        return Err(RvError::ErrResponse("missing CIDR list that needs validation".to_string()));
    }

    if separator.is_empty() {
        return Err(RvError::ErrResponse("missing separator".to_string()));
    }

    let cidrs_set: HashSet<&str> =
        cidr_list.split(separator).map(|cidr| cidr.trim()).filter(|cidr| !cidr.is_empty()).collect();

    let cidrs: Vec<&str> = cidrs_set.into_iter().collect();

    validate_cidrs(&cidrs)
}

pub fn validate_cidrs(cidrs: &[&str]) -> Result<bool, RvError> {
    if cidrs.is_empty() {
        return Err(RvError::ErrResponse("missing CIDR blocks that needs validation".to_string()));
    }

    for cidr in cidrs.iter() {
        let _ = IpNetwork::from_str(cidr)?;
    }

    Ok(true)
}

pub fn subset(cidr1: &str, cidr2: &str) -> Result<bool, RvError> {
    if cidr1.is_empty() {
        return Err(RvError::ErrResponse("missing CIDR to be checked against".to_string()));
    }

    if cidr2.is_empty() {
        return Err(RvError::ErrResponse("missing CIDR that needs to be checked".to_string()));
    }

    let ipnet1 = IpNetwork::from_str(cidr1)?;
    let mask_len1 = ipnet1.prefix();

    if !is_ip_addr_zero(&ipnet1.ip()) && mask_len1 == 0 {
        return Err(RvError::ErrResponse("CIDR to be checked against is not in its canonical form".to_string()));
    }

    let ipnet2 = IpNetwork::from_str(cidr2)?;
    let mask_len2 = ipnet2.prefix();

    if !is_ip_addr_zero(&ipnet2.ip()) && mask_len2 == 0 {
        return Err(RvError::ErrResponse("CIDR that needs to be checked is not in its canonical form".to_string()));
    }

    /*
     * If the mask length of the CIDR that needs to be checked is smaller
     * then the mask length of the CIDR to be checked against, then the
     * former will encompass more IPs than the latter, and hence can't be a
     * subset of the latter.
     */
    if mask_len2 < mask_len1 {
        return Ok(false);
    }

    Ok(ipnet1.contains(ipnet2.ip()))
}

/*
 * subset_blocks checks if each CIDR block of a given set of CIDR blocks, is a
 * subset of at least one CIDR block belonging to another set of CIDR blocks.
 * First parameter is the set of CIDR blocks to check against and the second
 * parameter is the set of CIDR blocks that needs to be checked.
 */
pub fn subset_blocks(cidr_blocks1: &[&str], cidr_blocks2: &[&str]) -> Result<bool, RvError> {
    if cidr_blocks1.is_empty() {
        return Err(RvError::ErrResponse("missing CIDR blocks to be checked against".to_string()));
    }

    if cidr_blocks2.is_empty() {
        return Err(RvError::ErrResponse("missing CIDR blocks that needs to be checked".to_string()));
    }

    // Check if all the elements of cidr_blocks2 is a subset of at least one
    // element of cidr_blocks1
    for cidr_block2 in cidr_blocks2.iter() {
        let mut is_subset = false;
        for cidr_block1 in cidr_blocks1.iter() {
            if subset(cidr_block1, cidr_block2)? {
                is_subset = true;
                break;
            }
        }

        if !is_subset {
            return Ok(false);
        }
    }

    Ok(true)
}

fn is_ip_addr_zero(ip_addr: &IpAddr) -> bool {
    match *ip_addr {
        IpAddr::V4(addr) => addr == Ipv4Addr::UNSPECIFIED,
        IpAddr::V6(addr) => addr == Ipv6Addr::UNSPECIFIED,
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_cidr_ip_belongs_to_cidr() {
        let ip = "192.168.25.30";
        let cidr = "192.168.25.30/16";
        let belongs = ip_belongs_to_cidr(ip, cidr);
        assert!(belongs.is_ok());
        assert!(belongs.unwrap());

        let ip = "10.197.192.6";
        let cidr = "10.197.192.0/18";
        let belongs = ip_belongs_to_cidr(ip, cidr);
        assert!(belongs.is_ok());
        assert!(belongs.unwrap());

        let ip = "192.168.25.30";
        let cidr = "192.168.26.30/24";
        let belongs = ip_belongs_to_cidr(ip, cidr);
        assert!(belongs.is_ok());
        assert!(!belongs.unwrap());

        let ip = "192.168.25.30.100";
        let cidr = "192.168.26.30/24";
        let belongs = ip_belongs_to_cidr(ip, cidr);
        assert!(belongs.is_err());
    }

    #[test]
    fn test_cidr_ip_belongs_to_cidrs() {
        let ip = "192.168.27.29";
        let cidrs = vec!["172.169.100.200/18", "192.168.0.0/16", "10.10.20.20/24"];
        let belongs = ip_belongs_to_cidrs(ip, &cidrs);
        assert!(belongs.is_ok());
        assert!(belongs.unwrap());

        let ip = "192.168.27.29";
        let cidrs = vec!["172.169.100.200/18", "192.168.0.0.0/16", "10.10.20.20/24"];
        let belongs = ip_belongs_to_cidrs(ip, &cidrs);
        assert!(belongs.is_err());

        let ip = "30.40.50.60";
        let cidrs = vec!["172.169.100.200/18", "192.168.0.0/16", "10.10.20.20/24"];
        let belongs = ip_belongs_to_cidrs(ip, &cidrs);
        assert!(belongs.is_ok());
        assert!(!belongs.unwrap());
    }

    #[test]
    fn test_cidr_validate_cidr_string() {
        let cidr = "172.169.100.200/18,192.168.0.0/16,10.10.20.20/24";
        let valid = validate_cidr_string(cidr, ",");
        assert!(valid.is_ok());
        assert!(valid.unwrap());

        let cidr = "172.169.100.200,192.168.0.0/16,10.10.20.20/24";
        let valid = validate_cidr_string(cidr, ",");
        assert!(valid.is_ok());
        assert!(valid.unwrap());

        let cidr = "172.169.100.200/18,192.168.0.0.0/16,10.10.20.20/24";
        let valid = validate_cidr_string(cidr, ",");
        assert!(valid.is_err());
    }

    #[test]
    fn test_cidr_validate_cidrs() {
        let cidrs = vec!["172.169.100.200/18", "192.168.0.0/16", "10.10.20.20/24"];
        let valid = validate_cidrs(&cidrs);
        assert!(valid.is_ok());
        assert!(valid.unwrap());

        let cidrs = vec!["172.169.100.200", "192.168.0.0/16", "10.10.20.20/24"];
        let valid = validate_cidrs(&cidrs);
        assert!(valid.is_ok());
        assert!(valid.unwrap());

        let cidrs = vec!["172.169.100.200/18", "192.168.0.0.0/16", "10.10.20.20/24"];
        let valid = validate_cidrs(&cidrs);
        assert!(valid.is_err());
    }

    #[test]
    fn test_cidr_subset() {
        let cidr1 = "192.168.27.29/24";
        let cidr2 = "192.168.27.29/24";
        let ret = subset(cidr1, cidr2);
        assert!(ret.is_ok());
        assert!(ret.unwrap());

        let cidr1 = "192.168.27.29/16";
        let cidr2 = "192.168.27.29/24";
        let ret = subset(cidr1, cidr2);
        assert!(ret.is_ok());
        assert!(ret.unwrap());
        let ret = subset(cidr2, cidr1);
        assert!(ret.is_ok());
        assert!(!ret.unwrap());

        let cidr1 = "192.168.0.128/25";
        let cidr2 = "192.168.0.0/24";
        let ret = subset(cidr1, cidr2);
        assert!(ret.is_ok());
        assert!(!ret.unwrap());
        let ret = subset(cidr2, cidr1);
        assert!(ret.is_ok());
        assert!(ret.unwrap());
    }

    #[test]
    fn test_cidr_subset_blocks() {
        let cidrs1 = vec!["192.168.27.29/16", "172.245.30.40/24", "10.20.30.40/30"];
        let cidrs2 = vec!["192.168.27.29/20", "172.245.30.40/25", "10.20.30.40/32"];
        let ret = subset_blocks(&cidrs1, &cidrs2);
        assert!(ret.is_ok());
        assert!(ret.unwrap());

        let cidrs1 = vec!["192.168.27.29/16", "172.245.30.40/25", "10.20.30.40/30"];
        let cidrs2 = vec!["192.168.27.29/20", "172.245.30.40/24", "10.20.30.40/32"];
        let ret = subset_blocks(&cidrs1, &cidrs2);
        assert!(ret.is_ok());
        assert!(!ret.unwrap());
    }

    #[test]
    fn test_cidr_remote_addr_is_ok() {
        let addr = new_sock_addr("127.0.0.1/8");
        assert!(addr.is_ok());
        let bound_cidrs = vec![addr.unwrap()];
        assert!(!remote_addr_is_ok("123.0.0.1", &bound_cidrs));
        assert!(remote_addr_is_ok("127.0.0.1", &bound_cidrs));
    }
}
