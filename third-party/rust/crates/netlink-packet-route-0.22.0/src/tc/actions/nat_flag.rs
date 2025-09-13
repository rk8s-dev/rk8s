// SPDX-License-Identifier: MIT

const TCA_NAT_FLAG_EGRESS: u32 = 1;
const TCA_ACT_FLAGS_USER_BITS: u32 = 16;
const TCA_ACT_FLAGS_POLICE: u32 = 1u32 << TCA_ACT_FLAGS_USER_BITS;
const TCA_ACT_FLAGS_BIND: u32 = 1u32 << (TCA_ACT_FLAGS_USER_BITS + 1);
const TCA_ACT_FLAGS_REPLACE: u32 = 1u32 << (TCA_ACT_FLAGS_USER_BITS + 2);
const TCA_ACT_FLAGS_NO_RTNL: u32 = 1u32 << (TCA_ACT_FLAGS_USER_BITS + 3);
const TCA_ACT_FLAGS_AT_INGRESS: u32 = 1u32 << (TCA_ACT_FLAGS_USER_BITS + 4);

bitflags! {
    /// Network Address Translation flags.
    #[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
    #[non_exhaustive]
    pub struct TcNatFlags: u32 {
        const Egress = TCA_NAT_FLAG_EGRESS;
        const Police = TCA_ACT_FLAGS_POLICE;
        const Bind = TCA_ACT_FLAGS_BIND;
        const Replace = TCA_ACT_FLAGS_REPLACE;
        const NoRtnl = TCA_ACT_FLAGS_NO_RTNL;
        const AtIngress = TCA_ACT_FLAGS_AT_INGRESS;
        const _ = !0;
    }
}
