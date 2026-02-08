use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

use crate::disk::{FileLockExt, Store};
use crate::range::IpRangeExt;
use crate::range_set::{RangeSet, RangeSetExt};

use ::cni_plugin::reply::Ip;
use anyhow::{anyhow, bail};
use ipnetwork::IpNetwork;

pub struct IpAllocator {
    range_set: RangeSet,
    store: Arc<Store>,
    range_id: String,
}

pub type TStore = Arc<Store>;

impl IpAllocator {
    pub fn new(range_set: RangeSet, store: TStore, id: usize) -> IpAllocator {
        IpAllocator {
            range_set,
            store,
            range_id: id.to_string(),
        }
    }
    pub fn get(&self, id: &str, if_name: &str, request_ip: Option<IpAddr>) -> anyhow::Result<Ip> {
        let _ = self.store.new_lock()?;
        let mut reserved_ip: Option<IpNetwork>;
        let mut gw: Option<IpAddr>;

        if let Some(request_ip) = request_ip {
            let r = self
                .range_set
                .iter()
                .find(|it| it.contains(request_ip))
                .ok_or(anyhow!(
                    "{} not in range set {}",
                    request_ip,
                    self.range_set.to_string()
                ))?;
            if request_ip == r.gateway.unwrap() {
                bail!("requested ip {} is subnet's gateway", request_ip);
            }

            let reserved = self
                .store
                .reserve(id, if_name, request_ip, &self.range_id)?;
            if !reserved {
                bail!(
                    "requested IP address {} is not available in range set {}",
                    request_ip,
                    self.range_set.to_string()
                );
            }

            reserved_ip = Some(IpNetwork::with_netmask(request_ip, r.subnet.mask()).unwrap());
            gw = Some(r.gateway.unwrap());
        } else {
            let allocated_ips = self.store.get_by_id(id, if_name)?;
            for ip in allocated_ips {
                if self.range_set.contains_ip(ip) {
                    bail!(
                        "{} has been allocated to {}, duplicate allocation is not allowed",
                        ip,
                        id
                    );
                }
            }
            let mut iter = self.new_iter();
            loop {
                match iter.next() {
                    None => {
                        reserved_ip = None;
                        gw = None;
                        break;
                    }
                    Some((ip, gw_)) => {
                        reserved_ip = Some(ip);
                        gw = Some(gw_);
                        if reserved_ip.is_none() {
                            break;
                        }
                        let reserved_ip = reserved_ip.unwrap();
                        let reserved =
                            self.store
                                .reserve(id, if_name, reserved_ip.ip(), &self.range_id)?;
                        if reserved {
                            break;
                        }
                    }
                }
            }
        }

        match (reserved_ip, gw) {
            (Some(reserved_ip), Some(gw)) => Ok(Ip {
                interface: None,
                address: reserved_ip,
                gateway: Some(gw),
            }),
            _ => {
                bail!(
                    "no IP addresses available in range set: {}",
                    self.range_set.to_string()
                );
            }
        }
    }
    pub fn release(&self, id: &str, ifname: &str) -> anyhow::Result<()> {
        let _ = self.store.new_lock()?;
        let _ = self.store.release_by_id(id, ifname)?;
        Ok(())
    }

    pub fn new_iter(&'_ self) -> Iter<'_> {
        let mut iter = Iter {
            range_set: &self.range_set,
            range_index: 0,
            cur: None,
            start_ip: None,
        };

        let mut start_from_last_reserved_ip = false;
        let mut last_reserved_ip: Option<IpAddr> = None;
        if let Some(ip) = self.store.last_reserved_ip(&self.range_id) {
            start_from_last_reserved_ip = self.range_set.contains_ip(ip);
            last_reserved_ip = Some(ip);
        };
        // Find the range in the set with this IP
        if start_from_last_reserved_ip {
            let last_reserved_ip = last_reserved_ip.unwrap();
            for (i, r) in self.range_set.iter().enumerate() {
                if r.contains(last_reserved_ip) {
                    iter.range_index = i;
                    iter.cur = Some(last_reserved_ip);
                    break;
                }
            }
        } else {
            iter.range_index = 0;
            iter.start_ip = self.range_set[0].range_start;
        }

        iter
    }
}

pub fn last_ip(subnet: &IpNetwork) -> IpAddr {
    match subnet {
        IpNetwork::V4(ip) => {
            let mask = ip.mask().octets();
            let mut ip = ip.ip().octets();
            for i in 0..4 {
                ip[i] |= !mask[i];
            }
            ip[3] -= 1;
            let end_ip = Ipv4Addr::from(ip);
            IpAddr::V4(end_ip)
        }
        IpNetwork::V6(_) => {
            todo!()
        }
    }
}

pub fn next_ip(ip: &IpAddr) -> Option<IpAddr> {
    match ip {
        IpAddr::V4(ipv4) => {
            let ip = ipv4.octets();
            let ip_num = u32::from_be_bytes(ip);
            let (ip_num, overflow) = ip_num.overflowing_add(1);
            if overflow {
                return None;
            }
            Some(IpAddr::V4(Ipv4Addr::from(ip_num.to_be_bytes())))
        }
        IpAddr::V6(_ipv6) => {
            todo!()
        }
    }
}

pub struct Iter<'a> {
    pub range_set: &'a RangeSet,
    pub range_index: usize,
    pub cur: Option<IpAddr>,
    pub start_ip: Option<IpAddr>,
}

impl Iterator for Iter<'_> {
    type Item = (IpNetwork, IpAddr);

    fn next(&mut self) -> Option<Self::Item> {
        let mut range = &self.range_set[self.range_index];

        // If this is the first time iterating and we're not starting in the middle
        // of the range, then start at rangeStart, which is inclusive
        if self.cur.is_none() {
            self.cur = range.range_start;
            self.start_ip = range.range_start;
            if self.cur == range.gateway {
                return self.next();
            }

            let ip = IpNetwork::with_netmask(range.range_start.unwrap(), range.subnet.mask()).ok();
            return Some((ip.unwrap(), range.gateway.unwrap()));
        }

        let cur = self.cur.unwrap();

        // If we've reached the end of this range, we need to advance the range
        // RangeEnd is inclusive as well
        if let Some(range_end) = range.range_end {
            if cur == range_end {
                self.range_index += 1;
                self.range_index %= self.range_set.len();
                range = &self.range_set[self.range_index];
                self.cur = range.range_start;
            } else {
                self.cur = next_ip(&cur);
            }
        } else {
            self.cur = next_ip(&cur);
        }

        if self.start_ip.is_none() {
            self.start_ip = self.cur;
        } else if self.cur == self.start_ip {
            // IF we've looped back to where we started, give up
            return None;
        }
        if self.cur == range.gateway {
            return self.next();
        }

        let ip = IpNetwork::with_netmask(self.cur.unwrap(), range.subnet.mask()).ok();
        Some((ip.unwrap(), range.gateway.unwrap()))
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use ipnetwork::Ipv4Network;

    use cni_plugin::ip_range::IpRange;

    use super::*;

    #[test]
    fn test_iter() {
        let range_set = vec![IpRange {
            range_start: Some(IpAddr::V4(Ipv4Addr::new(10, 10, 1, 20))),
            range_end: Some(IpAddr::V4(Ipv4Addr::new(10, 10, 3, 50))),
            subnet: IpNetwork::V4(Ipv4Network::new(Ipv4Addr::new(10, 10, 0, 0), 16).unwrap()),
            gateway: Some(IpAddr::V4(Ipv4Addr::new(10, 10, 0, 254))),
        }];
        let allocator = IpAllocator::new(
            range_set,
            Arc::new(Store::new(Some("/tmp".into())).unwrap()),
            1,
        );
        let mut iter = allocator.new_iter();
        let ip = iter.next();
        println!("{ip:?}")
    }

    #[test]
    fn test_last_ip() {
        let subnet = IpNetwork::new(IpAddr::V4(Ipv4Addr::new(10, 10, 0, 0)), 16).unwrap();
        let last = last_ip(&subnet);
        println!("{last:?}");
    }

    fn get_last_ip_in_subnet(subnet: &str) -> anyhow::Result<Ipv4Addr> {
        let (ip_str, prefix_str) = subnet
            .split_once('/')
            .ok_or("Invalid subnet format")
            .unwrap();
        let prefix: u32 = prefix_str.parse().unwrap();
        let mask = !((1 << (32 - prefix)) - 1);
        let ip = ip_str.parse::<Ipv4Addr>()?;
        let ip_u32 = u32::from(ip) & mask | (1 << (32 - prefix));
        let ip_u32 = ip_u32 - 2;
        Ok(Ipv4Addr::from(ip_u32))
    }

    #[test]
    fn test_1() {
        let subnet = "192.168.1.0/24";
        match get_last_ip_in_subnet(subnet) {
            Ok(ip) => println!("Last IP in the subnet is: {ip}"),
            Err(e) => println!("Error: {e}"),
        }
    }

    #[test]
    fn test_alloc() -> anyhow::Result<()> {
        let mut range_set = vec![IpRange {
            subnet: "192.168.1.0/29".parse().unwrap(),
            ..IpRangeExt::default_range()
        }];
        range_set.canonicalize().unwrap();
        std::fs::remove_dir_all("/tmp/ipam1").unwrap_or_default();
        let store = Arc::new(Store::new(Some("/tmp/ipam1".into())).unwrap());
        let alloc = IpAllocator::new(range_set, store, 1);

        for i in 2..7 {
            let ip = alloc.get(&format!("ID{i}"), "eth0", None)?;
            println!("{i}----{ip:?}");
            assert_eq!(ip.address.ip(), Ipv4Addr::new(192, 168, 1, i));
        }
        let result = alloc.get("ID8", "eth0", None);
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_2() {
        let mut range_set = vec![IpRange {
            subnet: "192.168.1.0/29".parse().unwrap(),
            ..IpRangeExt::default_range()
        }];
        range_set.canonicalize().unwrap();
        std::fs::remove_dir_all("/tmp/ipam2").unwrap_or_default();
        let store = Arc::new(Store::new(Some("/tmp/ipam2".into())).unwrap());
        let alloc = IpAllocator::new(range_set, store, 1);
        let res = alloc.get("ID", "eth0", None).unwrap();
        assert_eq!(res.address, "192.168.1.2/29".parse().unwrap());
        alloc.release("ID", "eth0").unwrap();

        let res = alloc.get("ID", "eth0", None).unwrap();
        assert_eq!(res.address, "192.168.1.3/29".parse().unwrap());
    }

    #[test]
    fn test_3() {
        {
            let mut range_set: RangeSet = vec![IpRange {
                subnet: "192.168.1.0/29".parse().unwrap(),
                ..IpRangeExt::default_range()
            }];
            range_set.canonicalize().unwrap();
            std::fs::remove_dir_all("/tmp/ipam3").unwrap_or_default();
            let store = Arc::new(Store::new(Some("/tmp/ipam3".into())).unwrap());
            let alloc = IpAllocator::new(range_set, store, 1);
            let mut iter = alloc.new_iter();
            assert_eq!(
                iter.next().unwrap().0.ip(),
                "192.168.1.2".parse::<IpAddr>().unwrap()
            );
            assert_eq!(
                iter.next().unwrap().0.ip(),
                "192.168.1.3".parse::<IpAddr>().unwrap()
            );
            assert_eq!(
                iter.next().unwrap().0.ip(),
                "192.168.1.4".parse::<IpAddr>().unwrap()
            );
            assert_eq!(
                iter.next().unwrap().0.ip(),
                "192.168.1.5".parse::<IpAddr>().unwrap()
            );
            assert_eq!(
                iter.next().unwrap().0.ip(),
                "192.168.1.6".parse::<IpAddr>().unwrap()
            );
            assert_eq!(iter.next(), None);
        }
        {
            let mut range_set: RangeSet = vec![IpRange {
                subnet: "192.168.1.0/29".parse().unwrap(),
                ..IpRangeExt::default_range()
            }];
            range_set.canonicalize().unwrap();
            std::fs::remove_dir_all("/tmp/ipam3").unwrap_or_default();
            let store = Arc::new(Store::new(Some("/tmp/ipam3".into())).unwrap());
            let alloc = IpAllocator::new(range_set, store, 1);
            let mut iter = alloc.new_iter();
            alloc
                .store
                .reserve("ID", "eth0", "192.168.1.6".parse::<IpAddr>().unwrap(), "1")
                .unwrap();
            alloc.store.release_by_id("ID", "eth0").unwrap();

            assert_eq!(
                iter.next().unwrap().0.ip(),
                "192.168.1.2".parse::<IpAddr>().unwrap()
            );
            assert_eq!(
                iter.next().unwrap().0.ip(),
                "192.168.1.3".parse::<IpAddr>().unwrap()
            );
            assert_eq!(
                iter.next().unwrap().0.ip(),
                "192.168.1.4".parse::<IpAddr>().unwrap()
            );
            assert_eq!(
                iter.next().unwrap().0.ip(),
                "192.168.1.5".parse::<IpAddr>().unwrap()
            );
            assert_eq!(
                iter.next().unwrap().0.ip(),
                "192.168.1.6".parse::<IpAddr>().unwrap()
            );
            assert_eq!(iter.next(), None);
        }

        {
            let mut range_set: RangeSet = vec![IpRange {
                subnet: "192.168.1.0/29".parse().unwrap(),
                ..IpRangeExt::default_range()
            }];
            range_set.canonicalize().unwrap();
            std::fs::remove_dir_all("/tmp/ipam3").unwrap_or_default();
            let store = Arc::new(Store::new(Some("/tmp/ipam3".into())).unwrap());
            let alloc = IpAllocator::new(range_set, store, 1);
            alloc
                .store
                .reserve("ID", "eth0", "192.168.1.3".parse::<IpAddr>().unwrap(), "1")
                .unwrap();
            alloc.store.release_by_id("ID", "eth0").unwrap();

            let mut iter = alloc.new_iter();
            assert_eq!(
                iter.next().unwrap().0.ip(),
                "192.168.1.4".parse::<IpAddr>().unwrap()
            );
            assert_eq!(
                iter.next().unwrap().0.ip(),
                "192.168.1.5".parse::<IpAddr>().unwrap()
            );
            assert_eq!(
                iter.next().unwrap().0.ip(),
                "192.168.1.6".parse::<IpAddr>().unwrap()
            );
            assert_eq!(
                iter.next().unwrap().0.ip(),
                "192.168.1.2".parse::<IpAddr>().unwrap()
            );
            assert_eq!(
                iter.next().unwrap().0.ip(),
                "192.168.1.3".parse::<IpAddr>().unwrap()
            );
            assert_eq!(iter.next(), None);
        }
    }

    #[test]
    fn test_4() {
        {
            let mut range_set: RangeSet = vec![IpRange {
                subnet: "10.0.0.0/29".parse().unwrap(),
                ..IpRangeExt::default_range()
            }];
            range_set.canonicalize().unwrap();
            std::fs::remove_dir_all("/tmp/ipam4").unwrap_or_default();
            let store = Arc::new(Store::new(Some("/tmp/ipam4".into())).unwrap());
            let alloc = IpAllocator::new(range_set, store, 1);
            let ip = alloc.get("ID", "eth0", None).unwrap();
            assert_eq!(ip.address.ip(), "10.0.0.2".parse::<IpAddr>().unwrap());
        }

        {
            let mut range_set: RangeSet = vec![IpRange {
                subnet: "10.0.0.0/30".parse().unwrap(),
                ..IpRangeExt::default_range()
            }];
            range_set.canonicalize().unwrap();
            std::fs::remove_dir_all("/tmp/ipam4").unwrap_or_default();
            let store = Arc::new(Store::new(Some("/tmp/ipam4".into())).unwrap());
            let alloc = IpAllocator::new(range_set, store, 1);
            let ip = alloc.get("ID", "eth0", None).unwrap();
            assert_eq!(ip.address.ip(), "10.0.0.2".parse::<IpAddr>().unwrap());
        }

        {
            let mut range_set: RangeSet = vec![IpRange {
                subnet: "10.0.0.0/29".parse().unwrap(),
                ..IpRangeExt::default_range()
            }];
            range_set.canonicalize().unwrap();
            std::fs::remove_dir_all("/tmp/ipam4").unwrap_or_default();
            let store = Arc::new(Store::new(Some("/tmp/ipam4".into())).unwrap());
            let alloc = IpAllocator::new(range_set, store, 1);
            let ip = alloc.get("ID1", "eth0", None).unwrap();
            assert_eq!(ip.address.ip(), "10.0.0.3".parse::<IpAddr>().unwrap());
        }
        {
            let mut range_set: RangeSet = vec![IpRange {
                subnet: "10.0.0.0/29".parse().unwrap(),
                ..IpRangeExt::default_range()
            }];
            range_set.canonicalize().unwrap();
            std::fs::remove_dir_all("/tmp/ipam4").unwrap_or_default();
            let store = Arc::new(Store::new(Some("/tmp/ipam4".into())).unwrap());
            let alloc = IpAllocator::new(range_set, store, 0);
            alloc
                .store
                .reserve("ID0", "eth0", "10.0.0.5".parse().unwrap(), "0")
                .unwrap();
            let ip = alloc.get("ID1", "eth0", None).unwrap();
            assert_eq!(ip.address.ip(), "10.0.0.6".parse::<IpAddr>().unwrap());
        }
        {
            let mut range_set: RangeSet = vec![IpRange {
                subnet: "10.0.0.0/29".parse().unwrap(),
                ..IpRangeExt::default_range()
            }];
            range_set.canonicalize().unwrap();
            std::fs::remove_dir_all("/tmp/ipam4").unwrap_or_default();
            let store = Arc::new(Store::new(Some("/tmp/ipam4".into())).unwrap());
            let alloc = IpAllocator::new(range_set, store, 0);
            alloc
                .store
                .reserve("ID0", "eth0", "10.0.0.4".parse().unwrap(), "0")
                .unwrap();
            alloc
                .store
                .reserve("ID0", "eth0", "10.0.0.5".parse().unwrap(), "0")
                .unwrap();
            alloc
                .store
                .reserve("ID0", "eth0", "10.0.0.3".parse().unwrap(), "0")
                .unwrap();
            let ip = alloc.get("ID1", "eth0", None).unwrap();
            assert_eq!(ip.address.ip(), "10.0.0.6".parse::<IpAddr>().unwrap());
        }
        {
            let mut range_set: RangeSet = vec![IpRange {
                subnet: "10.0.0.0/29".parse().unwrap(),
                ..IpRangeExt::default_range()
            }];
            range_set.canonicalize().unwrap();
            std::fs::remove_dir_all("/tmp/ipam4").unwrap_or_default();
            let store = Arc::new(Store::new(Some("/tmp/ipam4".into())).unwrap());
            let alloc = IpAllocator::new(range_set, store, 0);
            alloc
                .store
                .reserve("ID0", "eth0", "10.0.0.6".parse().unwrap(), "0")
                .unwrap();
            alloc
                .store
                .reserve("ID1", "eth0", "10.0.0.5".parse().unwrap(), "0")
                .unwrap();
            alloc.store.release_by_id("ID1", "eth0").unwrap();
            let ip = alloc.get("ID1", "eth0", None).unwrap();
            assert_eq!(ip.address.ip(), "10.0.0.2".parse::<IpAddr>().unwrap());
        }
        {
            let mut range_set: RangeSet = vec![IpRange {
                subnet: "10.0.0.0/29".parse().unwrap(),
                ..IpRangeExt::default_range()
            }];
            range_set.canonicalize().unwrap();
            std::fs::remove_dir_all("/tmp/ipam4").unwrap_or_default();
            let store = Arc::new(Store::new(Some("/tmp/ipam4".into())).unwrap());
            let alloc = IpAllocator::new(range_set, store, 0);
            alloc
                .store
                .reserve("ID2", "eth0", "10.0.0.2".parse().unwrap(), "0")
                .unwrap();
            alloc
                .store
                .reserve("ID4", "eth0", "10.0.0.4".parse().unwrap(), "0")
                .unwrap();
            alloc
                .store
                .reserve("ID5", "eth0", "10.0.0.5".parse().unwrap(), "0")
                .unwrap();
            alloc
                .store
                .reserve("ID6", "eth0", "10.0.0.6".parse().unwrap(), "0")
                .unwrap();
            alloc
                .store
                .reserve("ID3", "eth0", "10.0.0.3".parse().unwrap(), "0")
                .unwrap();
            alloc.store.release_by_id("ID3", "eth0").unwrap();
            let ip = alloc.get("ID3", "eth0", None).unwrap();
            assert_eq!(ip.address.ip(), "10.0.0.3".parse::<IpAddr>().unwrap());
        }
        // advance to next subnet
        {
            let mut range_set: RangeSet = vec![
                IpRange {
                    subnet: "10.0.0.0/30".parse().unwrap(),
                    ..IpRangeExt::default_range()
                },
                IpRange {
                    subnet: "10.0.1.0/30".parse().unwrap(),
                    ..IpRangeExt::default_range()
                },
            ];
            range_set.canonicalize().unwrap();
            std::fs::remove_dir_all("/tmp/ipam4").unwrap_or_default();
            let store = Arc::new(Store::new(Some("/tmp/ipam4".into())).unwrap());
            let alloc = IpAllocator::new(range_set, store, 0);
            alloc
                .store
                .reserve("ID2", "eth0", "10.0.0.2".parse().unwrap(), "0")
                .unwrap();
            alloc.store.release_by_id("ID2", "eth0").unwrap();
            let ip = alloc.get("ID3", "eth0", None).unwrap();
            assert_eq!(ip.address.ip(), "10.0.1.2".parse::<IpAddr>().unwrap());
        }
        {
            let mut range_set: RangeSet = vec![
                IpRange {
                    subnet: "10.0.0.0/30".parse().unwrap(),
                    ..IpRangeExt::default_range()
                },
                IpRange {
                    subnet: "10.0.1.0/30".parse().unwrap(),
                    ..IpRangeExt::default_range()
                },
                IpRange {
                    subnet: "10.0.2.0/30".parse().unwrap(),
                    ..IpRangeExt::default_range()
                },
            ];
            range_set.canonicalize().unwrap();
            std::fs::remove_dir_all("/tmp/ipam4").unwrap_or_default();
            let store = Arc::new(Store::new(Some("/tmp/ipam4".into())).unwrap());
            let alloc = IpAllocator::new(range_set, store, 0);
            alloc
                .store
                .reserve("ID2", "eth0", "10.0.2.2".parse().unwrap(), "0")
                .unwrap();
            alloc.store.release_by_id("ID2", "eth0").unwrap();
            let ip = alloc.get("ID3", "eth0", None).unwrap();
            assert_eq!(ip.address.ip(), "10.0.0.2".parse::<IpAddr>().unwrap());
        }
    }

    #[test]
    fn test_should_not_broadcast() {
        let mut range_set: RangeSet = vec![IpRange {
            subnet: "10.0.0.0/29".parse().unwrap(),
            ..IpRangeExt::default_range()
        }];
        range_set.canonicalize().unwrap();
        std::fs::remove_dir_all("/tmp/ipam5").unwrap_or_default();
        let store = Arc::new(Store::new(Some("/tmp/ipam5".into())).unwrap());
        let alloc = IpAllocator::new(range_set, store, 0);
        for i in 2..7 {
            let ip = alloc.get(&format!("ID{i}"), "eth0", None).unwrap();
            assert_eq!(ip.address.ip(), Ipv4Addr::new(10, 0, 0, i));
        }
        let result = alloc.get("ID8", "eth0", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_specific_ip() {
        let mut range_set: RangeSet = vec![IpRange {
            subnet: "192.168.1.0/29".parse().unwrap(),
            ..IpRangeExt::default_range()
        }];
        range_set.canonicalize().unwrap();
        std::fs::remove_dir_all("/tmp/ipam6").unwrap_or_default();
        let store = Arc::new(Store::new(Some("/tmp/ipam6".into())).unwrap());
        let alloc = IpAllocator::new(range_set, store, 0);
        let ip = alloc
            .get("ID", "eth0", Some("192.168.1.5".parse().unwrap()))
            .unwrap();
        assert_eq!(ip.address.ip(), "192.168.1.5".parse::<IpAddr>().unwrap());
        let result = alloc.get("ID", "eth0", Some("192.168.1.5".parse().unwrap()));
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "requested IP address 192.168.1.5 is not available in range set 192.168.1.1-192.168.1.6"
        );
    }

    #[test]
    fn test_after_range() {
        let mut range_set: RangeSet = vec![IpRange {
            subnet: "192.168.1.0/29".parse().unwrap(),
            range_end: Some("192.168.1.4".parse().unwrap()),
            ..IpRangeExt::default_range()
        }];
        range_set.canonicalize().unwrap();
        std::fs::remove_dir_all("/tmp/ipam7").unwrap_or_default();
        let store = Arc::new(Store::new(Some("/tmp/ipam7".into())).unwrap());
        let alloc = IpAllocator::new(range_set, store, 0);
        let result = alloc.get("ID", "eth0", Some("192.168.1.5".parse().unwrap()));
        assert!(result.is_err());

        let mut range_set: RangeSet = vec![IpRange {
            subnet: "192.168.1.0/29".parse().unwrap(),
            range_start: Some("192.168.1.3".parse().unwrap()),
            ..IpRangeExt::default_range()
        }];
        range_set.canonicalize().unwrap();
        std::fs::remove_dir_all("/tmp/ipam7").unwrap_or_default();
        let store = Arc::new(Store::new(Some("/tmp/ipam7".into())).unwrap());
        let alloc = IpAllocator::new(range_set, store, 0);
        let result = alloc.get("ID", "eth0", Some("192.168.1.2".parse().unwrap()));
        assert!(result.is_err());
    }
}
