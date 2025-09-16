// SPDX-License-Identifier: MIT

const FR_ACT_UNSPEC: u8 = 0;
const FR_ACT_TO_TBL: u8 = 1;
const FR_ACT_GOTO: u8 = 2;
const FR_ACT_NOP: u8 = 3;
// const FR_ACT_RES3: u8 = 4;
// const FR_ACT_RES4: u8 = 5;
const FR_ACT_BLACKHOLE: u8 = 6;
const FR_ACT_UNREACHABLE: u8 = 7;
const FR_ACT_PROHIBIT: u8 = 8;

#[derive(Eq, PartialEq, Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub enum RuleAction {
    #[default]
    Unspec,
    ToTable,
    Goto,
    Nop,
    Blackhole,
    Unreachable,
    Prohibit,
    Other(u8),
}

impl From<u8> for RuleAction {
    fn from(d: u8) -> Self {
        match d {
            FR_ACT_UNSPEC => Self::Unspec,
            FR_ACT_TO_TBL => Self::ToTable,
            FR_ACT_GOTO => Self::Goto,
            FR_ACT_NOP => Self::Nop,
            FR_ACT_BLACKHOLE => Self::Blackhole,
            FR_ACT_UNREACHABLE => Self::Unreachable,
            FR_ACT_PROHIBIT => Self::Prohibit,
            _ => Self::Other(d),
        }
    }
}

impl From<RuleAction> for u8 {
    fn from(v: RuleAction) -> u8 {
        match v {
            RuleAction::Unspec => FR_ACT_UNSPEC,
            RuleAction::ToTable => FR_ACT_TO_TBL,
            RuleAction::Goto => FR_ACT_GOTO,
            RuleAction::Nop => FR_ACT_NOP,
            RuleAction::Blackhole => FR_ACT_BLACKHOLE,
            RuleAction::Unreachable => FR_ACT_UNREACHABLE,
            RuleAction::Prohibit => FR_ACT_PROHIBIT,
            RuleAction::Other(d) => d,
        }
    }
}
