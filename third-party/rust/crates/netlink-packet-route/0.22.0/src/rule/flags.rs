// SPDX-License-Identifier: MIT

const FIB_RULE_PERMANENT: u32 = 0x00000001;
const FIB_RULE_INVERT: u32 = 0x00000002;
const FIB_RULE_UNRESOLVED: u32 = 0x00000004;
const FIB_RULE_IIF_DETACHED: u32 = 0x00000008;
const FIB_RULE_DEV_DETACHED: u32 = FIB_RULE_IIF_DETACHED;
const FIB_RULE_OIF_DETACHED: u32 = 0x00000010;

bitflags! {
    #[derive(Clone, Eq, PartialEq, Debug, Copy, Default)]
    #[non_exhaustive]
    pub struct RuleFlags: u32 {
        const Permanent = FIB_RULE_PERMANENT;
        const Invert = FIB_RULE_INVERT;
        const Unresolved = FIB_RULE_UNRESOLVED;
        const IifDetached = FIB_RULE_IIF_DETACHED;
        const DevDetached = FIB_RULE_DEV_DETACHED;
        const OifDetached = FIB_RULE_OIF_DETACHED;
        const _ = !0;
    }
}
