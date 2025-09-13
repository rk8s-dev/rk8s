// SPDX-License-Identifier: MIT

const ETH_P_8021Q: u16 = 0x8100;
const ETH_P_8021AD: u16 = 0x88A8;

#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
#[non_exhaustive]
#[repr(u16)]
// VLAN protocol seldom add new, so no Other for this enum.
pub enum VlanProtocol {
    #[default]
    Ieee8021Q = ETH_P_8021Q,
    Ieee8021Ad = ETH_P_8021AD,
}

impl From<u16> for VlanProtocol {
    fn from(d: u16) -> Self {
        match d {
            ETH_P_8021Q => Self::Ieee8021Q,
            ETH_P_8021AD => Self::Ieee8021Ad,
            _ => {
                log::warn!(
                    "BUG: Got unknown VLAN protocol {}, treating as {}",
                    d,
                    Self::Ieee8021Q
                );
                Self::Ieee8021Q
            }
        }
    }
}

impl From<VlanProtocol> for u16 {
    fn from(v: VlanProtocol) -> u16 {
        match v {
            VlanProtocol::Ieee8021Q => ETH_P_8021Q,
            VlanProtocol::Ieee8021Ad => ETH_P_8021AD,
        }
    }
}

impl std::fmt::Display for VlanProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                VlanProtocol::Ieee8021Q => "802.1q",
                VlanProtocol::Ieee8021Ad => "802.1ad",
            }
        )
    }
}
