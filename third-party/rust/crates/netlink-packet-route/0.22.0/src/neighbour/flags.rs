// SPDX-License-Identifier: MIT

const NTF_USE: u8 = 1 << 0;
const NTF_SELF: u8 = 1 << 1;
// Kernel constant name is NTF_MASTER
const NTF_CONTROLLER: u8 = 1 << 2;
const NTF_PROXY: u8 = 1 << 3;
const NTF_EXT_LEARNED: u8 = 1 << 4;
const NTF_OFFLOADED: u8 = 1 << 5;
const NTF_STICKY: u8 = 1 << 6;
const NTF_ROUTER: u8 = 1 << 7;

bitflags! {
    #[derive(Clone, Eq, PartialEq, Debug, Copy, Default)]
    #[non_exhaustive]
    pub struct NeighbourFlags: u8 {
        const Use = NTF_USE;
        const Own = NTF_SELF;
        const Controller = NTF_CONTROLLER;
        const Proxy = NTF_PROXY;
        const ExtLearned = NTF_EXT_LEARNED;
        const Offloaded = NTF_OFFLOADED;
        const Sticky = NTF_STICKY;
        const Router = NTF_ROUTER;
        const _ = !0;
    }
}
