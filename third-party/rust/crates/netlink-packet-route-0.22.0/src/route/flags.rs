// SPDX-License-Identifier: MIT

use super::next_hops::{
    RTNH_F_DEAD, RTNH_F_LINKDOWN, RTNH_F_OFFLOAD, RTNH_F_ONLINK,
    RTNH_F_PERVASIVE, RTNH_F_TRAP, RTNH_F_UNRESOLVED,
};

const RTM_F_NOTIFY: u32 = 0x100;
const RTM_F_CLONED: u32 = 0x200;
const RTM_F_EQUALIZE: u32 = 0x400;
const RTM_F_PREFIX: u32 = 0x800;
const RTM_F_LOOKUP_TABLE: u32 = 0x1000;
const RTM_F_FIB_MATCH: u32 = 0x2000;
const RTM_F_OFFLOAD: u32 = 0x4000;
const RTM_F_TRAP: u32 = 0x8000;
const RTM_F_OFFLOAD_FAILED: u32 = 0x20000000;

bitflags! {
    #[derive(Clone, Eq, PartialEq, Debug, Copy, Default)]
    #[non_exhaustive]
    pub struct RouteFlags: u32 {
        const Dead = RTNH_F_DEAD as u32;
        const Pervasive = RTNH_F_PERVASIVE as u32;
        const Onlink = RTNH_F_ONLINK as u32;
        const Offload = RTNH_F_OFFLOAD as u32;
        const Linkdown = RTNH_F_LINKDOWN as u32;
        const Unresolved = RTNH_F_UNRESOLVED as u32;
        const Trap = RTNH_F_TRAP as u32;
        const Notify = RTM_F_NOTIFY;
        const Cloned = RTM_F_CLONED;
        const Equalize = RTM_F_EQUALIZE;
        const Prefix = RTM_F_PREFIX;
        const LookupTable = RTM_F_LOOKUP_TABLE;
        const FibMatch = RTM_F_FIB_MATCH;
        const RtOffload = RTM_F_OFFLOAD;
        const RtTrap = RTM_F_TRAP;
        const OffloadFailed = RTM_F_OFFLOAD_FAILED;
        const _ = !0;
    }
}
