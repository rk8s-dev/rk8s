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

#[derive(Clone, Eq, PartialEq, Debug, Copy, Default)]
#[non_exhaustive]
pub enum NeighbourState {
    Incomplete,
    Reachable,
    Stale,
    Delay,
    Probe,
    Failed,
    Noarp,
    Permanent,
    #[default]
    None,
    Other(u16),
}

impl From<NeighbourState> for u16 {
    fn from(v: NeighbourState) -> u16 {
        match v {
            NeighbourState::Incomplete => NUD_INCOMPLETE,
            NeighbourState::Reachable => NUD_REACHABLE,
            NeighbourState::Stale => NUD_STALE,
            NeighbourState::Delay => NUD_DELAY,
            NeighbourState::Probe => NUD_PROBE,
            NeighbourState::Failed => NUD_FAILED,
            NeighbourState::Noarp => NUD_NOARP,
            NeighbourState::Permanent => NUD_PERMANENT,
            NeighbourState::None => NUD_NONE,
            NeighbourState::Other(d) => d,
        }
    }
}

impl From<u16> for NeighbourState {
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

impl std::fmt::Display for NeighbourState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Incomplete => write!(f, "incomplete"),
            Self::Reachable => write!(f, "reachable"),
            Self::Stale => write!(f, "stale"),
            Self::Delay => write!(f, "delay"),
            Self::Probe => write!(f, "probe"),
            Self::Failed => write!(f, "failed"),
            Self::Noarp => write!(f, "noarp"),
            Self::Permanent => write!(f, "permanent"),
            Self::None => write!(f, "none"),
            Self::Other(d) => write!(f, "other({d}"),
        }
    }
}
