// SPDX-License-Identifier: MIT

const IF_RA_OTHERCONF: u32 = 0x80;
const IF_RA_MANAGED: u32 = 0x40;
const IF_RA_RCVD: u32 = 0x20;
const IF_RS_SENT: u32 = 0x10;
const IF_READY: u32 = 0x80000000;

bitflags! {
    #[derive(Debug, PartialEq, Eq, Clone, Copy)]
    #[non_exhaustive]
    pub struct Inet6IfaceFlags : u32 {
        const Otherconf = IF_RA_OTHERCONF;
        const RaManaged = IF_RA_MANAGED;
        const RaRcvd = IF_RA_RCVD;
        const RsSent = IF_RS_SENT;
        const Ready = IF_READY;
        const _ = !0;
    }
}
