// SPDX-License-Identifier: MIT

const TC_U32_TERMINAL: u8 = 1;
const TC_U32_OFFSET: u8 = 2;
const TC_U32_VAROFFSET: u8 = 4;
const TC_U32_EAT: u8 = 8;

bitflags! {
    #[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
    #[non_exhaustive]
    pub struct TcU32SelectorFlags: u8 {
        const Terminal = TC_U32_TERMINAL;
        const Offset = TC_U32_OFFSET;
        const VarOffset = TC_U32_VAROFFSET;
        const Eat = TC_U32_EAT;
        const _ = !0;
    }
}

const TCA_CLS_FLAGS_SKIP_HW: u32 = 1 << 0;
const TCA_CLS_FLAGS_SKIP_SW: u32 = 1 << 1;
const TCA_CLS_FLAGS_IN_HW: u32 = 1 << 2;
const TCA_CLS_FLAGS_NOT_IN_HW: u32 = 1 << 3;
const TCA_CLS_FLAGS_VERBOSE: u32 = 1 << 4;

bitflags! {
    #[derive(Debug, PartialEq, Eq, Clone, Copy)]
    #[non_exhaustive]
    pub struct TcU32OptionFlags: u32 {
        const SkipHw = TCA_CLS_FLAGS_SKIP_HW;
        const SkipSw = TCA_CLS_FLAGS_SKIP_SW;
        const InHw = TCA_CLS_FLAGS_IN_HW;
        const NotInHw = TCA_CLS_FLAGS_NOT_IN_HW;
        const Verbose = TCA_CLS_FLAGS_VERBOSE;
        const _ = !0;
    }
}
