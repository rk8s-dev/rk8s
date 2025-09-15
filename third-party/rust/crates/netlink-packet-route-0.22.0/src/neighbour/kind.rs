// SPDX-License-Identifier: MIT

const NUD_INCOMPLETE: u16 = 0x01;
const NUD_REACHABLE: u16 = 0x02;
const NUD_STALE: u16 = 0x04;
const NUD_DELAY: u16 = 0x08;
const NUD_PROBE: u16 = 0x10;
const NUD_FAILED: u16 = 0x20;
const NUD_NOARP: u16 = 0x40;
const NUD_PERMANENT: u16 = 0x80;
const NUD_NONE: u16 = 0x00;

/// Types that can be set in a `RTM_GETROUTE`
/// ([`NeightbourNetlinkMessage::GetNeightbour`]) message.
#[derive(Clone, Eq, PartialEq, Debug, Copy)]
#[non_exhaustive]
pub enum NeightbourType {
    Incomplete,
    Reachable,
    Stale,
    Delay,
    Probe,
    Failed,
    Noarp,
    Permanent,
    None,
    Other(u16),
}

impl From<NeightbourType> for u16 {
    fn from(v: NeightbourType) -> u16 {
        match v {
            NeightbourType::Incomplete => NUD_INCOMPLETE,
            NeightbourType::Reachable => NUD_REACHABLE,
            NeightbourType::Stale => NUD_STALE,
            NeightbourType::Delay => NUD_DELAY,
            NeightbourType::Probe => NUD_PROBE,
            NeightbourType::Failed => NUD_FAILED,
            NeightbourType::Noarp => NUD_NOARP,
            NeightbourType::Permanent => NUD_PERMANENT,
            NeightbourType::None => NUD_NONE,
            NeightbourType::Other(d) => d,
        }
    }
}

impl From<u16> for NeightbourType {
    fn from(d: u16) -> Self {
        match d {
            NUD_INCOMPLETE => Self::Incomplete,
            NUD_REACHABLE => Self::Reachable,
            NUD_STALE => Self::Stale,
            NUD_DELAY => Self::Delay,
            NUD_PROBE => Self::Probe,
            NUD_FAILED => Self::Failed,
            NUD_NOARP => Self::Noarp,
            NUD_PERMANENT => Self::Permanent,
            NUD_NONE => Self::None,
            _ => Self::Other(d),
        }
    }
}
