use crate::range::IpRangeExt;
use std::net::IpAddr;

use anyhow::bail;
use cni_plugin::ip_range::IpRange;

pub type RangeSet = Vec<IpRange>;

pub trait RangeSetExt {
    fn canonicalize(&mut self) -> anyhow::Result<()>;
    fn contains_ip(&self, ip_addr: IpAddr) -> bool;
    fn overlap(&self, other: &RangeSet) -> bool;
    fn to_string(&self) -> String;
}

impl RangeSetExt for RangeSet {
    fn canonicalize(&mut self) -> anyhow::Result<()> {
        if self.is_empty() {
            bail!("empty range set")
        }

        for range in self.iter_mut() {
            range.canonicalize()?;
        }

        // Make sure none of the ranges in the set overlap
        let n = self.len();
        for i in 0..n {
            for j in i + 1..n {
                if self[i].overlap(&self[j]) {
                    bail!("subnets {:?} and {:?} overlap", self[i], self[j])
                }
            }
        }
        Ok(())
    }
    fn contains_ip(&self, ip_addr: IpAddr) -> bool {
        self.iter().any(|range| range.contains(ip_addr))
    }

    // Overlaps returns true if any ranges in any set overlap with this one
    fn overlap(&self, other: &RangeSet) -> bool {
        for r1 in self.iter() {
            for r2 in other.iter() {
                if r1.overlap(r2) {
                    return true;
                }
            }
        }
        false
    }

    fn to_string(&self) -> String {
        self.iter()
            .map(|it| it.format())
            .collect::<Vec<_>>()
            .join(",")
    }
}
