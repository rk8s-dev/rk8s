// SPDX-License-Identifier: MIT

const IF_OPER_UNKNOWN: u8 = 0;
const IF_OPER_NOTPRESENT: u8 = 1;
const IF_OPER_DOWN: u8 = 2;
const IF_OPER_LOWERLAYERDOWN: u8 = 3;
const IF_OPER_TESTING: u8 = 4;
const IF_OPER_DORMANT: u8 = 5;
const IF_OPER_UP: u8 = 6;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[non_exhaustive]
pub enum State {
    /// Status can't be determined
    Unknown,
    /// Some component is missing
    NotPresent,
    /// Down
    Down,
    /// Down due to state of lower layer
    LowerLayerDown,
    /// In some test mode
    Testing,
    /// Not up but pending an external event
    Dormant,
    /// Up, ready to send packets
    Up,
    /// Place holder for new state introduced by kernel when current crate does
    /// not support so.
    Other(u8),
}

impl From<u8> for State {
    fn from(value: u8) -> Self {
        use self::State::*;
        match value {
            IF_OPER_UNKNOWN => Unknown,
            IF_OPER_NOTPRESENT => NotPresent,
            IF_OPER_DOWN => Down,
            IF_OPER_LOWERLAYERDOWN => LowerLayerDown,
            IF_OPER_TESTING => Testing,
            IF_OPER_DORMANT => Dormant,
            IF_OPER_UP => Up,
            _ => Other(value),
        }
    }
}

impl From<State> for u8 {
    fn from(value: State) -> Self {
        use self::State::*;
        match value {
            Unknown => IF_OPER_UNKNOWN,
            NotPresent => IF_OPER_NOTPRESENT,
            Down => IF_OPER_DOWN,
            LowerLayerDown => IF_OPER_LOWERLAYERDOWN,
            Testing => IF_OPER_TESTING,
            Dormant => IF_OPER_DORMANT,
            Up => IF_OPER_UP,
            Other(other) => other,
        }
    }
}
