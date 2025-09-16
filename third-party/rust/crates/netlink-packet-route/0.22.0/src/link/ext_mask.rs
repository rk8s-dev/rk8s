// SPDX-License-Identifier: MIT

const RTEXT_FILTER_VF: u32 = 1 << 0;
const RTEXT_FILTER_BRVLAN: u32 = 1 << 1;
const RTEXT_FILTER_BRVLAN_COMPRESSED: u32 = 1 << 2;
const RTEXT_FILTER_SKIP_STATS: u32 = 1 << 3;
const RTEXT_FILTER_MRP: u32 = 1 << 4;
const RTEXT_FILTER_CFM_CONFIG: u32 = 1 << 5;
const RTEXT_FILTER_CFM_STATUS: u32 = 1 << 6;
const RTEXT_FILTER_MST: u32 = 1 << 7;

#[derive(Debug, PartialEq, Eq, Clone, Default)]
pub(crate) struct VecLinkExtentMask(pub(crate) Vec<LinkExtentMask>);

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[non_exhaustive]
#[repr(u32)]
pub enum LinkExtentMask {
    Vf,
    Brvlan,
    BrvlanCompressed,
    SkipStats,
    Mrp,
    CfmConfig,
    CfmStatus,
    Mst,
    Other(u32),
}

impl From<u32> for LinkExtentMask {
    fn from(d: u32) -> Self {
        match d {
            RTEXT_FILTER_VF => Self::Vf,
            RTEXT_FILTER_BRVLAN => Self::Brvlan,
            RTEXT_FILTER_BRVLAN_COMPRESSED => Self::BrvlanCompressed,
            RTEXT_FILTER_SKIP_STATS => Self::SkipStats,
            RTEXT_FILTER_MRP => Self::Mrp,
            RTEXT_FILTER_CFM_CONFIG => Self::CfmConfig,
            RTEXT_FILTER_CFM_STATUS => Self::CfmStatus,
            RTEXT_FILTER_MST => Self::Mst,
            _ => Self::Other(d),
        }
    }
}

impl From<LinkExtentMask> for u32 {
    fn from(v: LinkExtentMask) -> u32 {
        match v {
            LinkExtentMask::Vf => RTEXT_FILTER_VF,
            LinkExtentMask::Brvlan => RTEXT_FILTER_BRVLAN,
            LinkExtentMask::BrvlanCompressed => RTEXT_FILTER_BRVLAN_COMPRESSED,
            LinkExtentMask::SkipStats => RTEXT_FILTER_SKIP_STATS,
            LinkExtentMask::Mrp => RTEXT_FILTER_MRP,
            LinkExtentMask::CfmConfig => RTEXT_FILTER_CFM_CONFIG,
            LinkExtentMask::CfmStatus => RTEXT_FILTER_CFM_STATUS,
            LinkExtentMask::Mst => RTEXT_FILTER_MST,
            LinkExtentMask::Other(i) => i,
        }
    }
}

const ALL_LINK_FLAGS: [LinkExtentMask; 8] = [
    LinkExtentMask::Vf,
    LinkExtentMask::Brvlan,
    LinkExtentMask::BrvlanCompressed,
    LinkExtentMask::SkipStats,
    LinkExtentMask::Mrp,
    LinkExtentMask::CfmConfig,
    LinkExtentMask::CfmStatus,
    LinkExtentMask::Mst,
];

impl From<u32> for VecLinkExtentMask {
    fn from(d: u32) -> Self {
        let mut got: u32 = 0;
        let mut ret = Vec::new();
        for flag in ALL_LINK_FLAGS {
            if (d & u32::from(flag)) > 0 {
                ret.push(flag);
                got += u32::from(flag);
            }
        }
        if got != d {
            ret.push(LinkExtentMask::Other(d - got));
        }
        Self(ret)
    }
}

impl From<&VecLinkExtentMask> for u32 {
    fn from(v: &VecLinkExtentMask) -> u32 {
        let mut d: u32 = 0;
        for flag in &v.0 {
            d += u32::from(*flag);
        }
        d
    }
}
