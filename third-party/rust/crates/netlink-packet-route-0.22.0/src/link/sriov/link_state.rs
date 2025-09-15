// SPDX-License-Identifier: MIT

use netlink_packet_utils::{DecodeError, Emitable, Parseable};

const VF_INFO_LINK_STATE_LEN: usize = 8;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
#[non_exhaustive]
pub struct VfInfoLinkState {
    pub vf_id: u32,
    pub state: VfLinkState,
}

impl VfInfoLinkState {
    pub fn new(vf_id: u32, state: VfLinkState) -> Self {
        Self { vf_id, state }
    }
}

buffer!(VfInfoLinkStateBuffer(VF_INFO_LINK_STATE_LEN) {
    vf_id: (u32, 0..4),
    state: (u32, 4..8),
});

impl<T: AsRef<[u8]> + ?Sized> Parseable<VfInfoLinkStateBuffer<&T>>
    for VfInfoLinkState
{
    fn parse(buf: &VfInfoLinkStateBuffer<&T>) -> Result<Self, DecodeError> {
        Ok(Self::new(buf.vf_id(), buf.state().into()))
    }
}

impl Emitable for VfInfoLinkState {
    fn buffer_len(&self) -> usize {
        VF_INFO_LINK_STATE_LEN
    }

    fn emit(&self, buffer: &mut [u8]) {
        let mut buffer = VfInfoLinkStateBuffer::new(buffer);
        buffer.set_vf_id(self.vf_id);
        buffer.set_state(self.state.into());
    }
}

const IFLA_VF_LINK_STATE_AUTO: u32 = 0;
const IFLA_VF_LINK_STATE_ENABLE: u32 = 1;
const IFLA_VF_LINK_STATE_DISABLE: u32 = 2;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
#[non_exhaustive]
pub enum VfLinkState {
    #[default]
    Auto,
    Enable,
    Disable,
    Other(u32),
}

impl From<u32> for VfLinkState {
    fn from(d: u32) -> Self {
        match d {
            IFLA_VF_LINK_STATE_AUTO => Self::Auto,
            IFLA_VF_LINK_STATE_ENABLE => Self::Enable,
            IFLA_VF_LINK_STATE_DISABLE => Self::Disable,
            _ => Self::Other(d),
        }
    }
}

impl From<VfLinkState> for u32 {
    fn from(v: VfLinkState) -> u32 {
        match v {
            VfLinkState::Auto => IFLA_VF_LINK_STATE_AUTO,
            VfLinkState::Enable => IFLA_VF_LINK_STATE_ENABLE,
            VfLinkState::Disable => IFLA_VF_LINK_STATE_DISABLE,
            VfLinkState::Other(d) => d,
        }
    }
}
