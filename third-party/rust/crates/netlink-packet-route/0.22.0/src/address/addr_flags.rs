// SPDX-License-Identifier: MIT

const IFA_F_SECONDARY: u32 = 0x01;
const IFA_F_NODAD: u32 = 0x02;
const IFA_F_OPTIMISTIC: u32 = 0x04;
const IFA_F_DADFAILED: u32 = 0x08;
const IFA_F_HOMEADDRESS: u32 = 0x10;
const IFA_F_DEPRECATED: u32 = 0x20;
const IFA_F_TENTATIVE: u32 = 0x40;
const IFA_F_PERMANENT: u32 = 0x80;
const IFA_F_MANAGETEMPADDR: u32 = 0x100;
const IFA_F_NOPREFIXROUTE: u32 = 0x200;
const IFA_F_MCAUTOJOIN: u32 = 0x400;
const IFA_F_STABLE_PRIVACY: u32 = 0x800;

bitflags! {
    #[non_exhaustive]
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct AddressFlags : u32 {
        const Secondary = IFA_F_SECONDARY;
        const Nodad = IFA_F_NODAD;
        const Optimistic = IFA_F_OPTIMISTIC;
        const Dadfailed = IFA_F_DADFAILED;
        const Homeaddress = IFA_F_HOMEADDRESS;
        const Deprecated = IFA_F_DEPRECATED;
        const Tentative = IFA_F_TENTATIVE;
        const Permanent = IFA_F_PERMANENT;
        const Managetempaddr = IFA_F_MANAGETEMPADDR;
        const Noprefixroute = IFA_F_NOPREFIXROUTE;
        const Mcautojoin = IFA_F_MCAUTOJOIN;
        const StablePrivacy = IFA_F_STABLE_PRIVACY;
        const _ = !0;
    }
}

bitflags! {
    #[non_exhaustive]
    #[derive(Debug, Clone, Copy, Eq, PartialEq)]
    /// [`AddressHeaderFlags`] is only used for [`super::AddressHeader`] and holding
    /// subset(first byte) of [`AddressFlags`].
    pub struct AddressHeaderFlags : u8 {
        const Secondary = IFA_F_SECONDARY as u8;
        const Nodad = IFA_F_NODAD as u8;
        const Optimistic = IFA_F_OPTIMISTIC as u8;
        const Dadfailed = IFA_F_DADFAILED as u8;
        const Homeaddress = IFA_F_HOMEADDRESS as u8;
        const Deprecated = IFA_F_DEPRECATED as u8;
        const Tentative = IFA_F_TENTATIVE as u8;
        const Permanent = IFA_F_PERMANENT as u8;
        const _ = !0;
    }
}

impl Default for AddressHeaderFlags {
    fn default() -> Self {
        Self::empty()
    }
}
