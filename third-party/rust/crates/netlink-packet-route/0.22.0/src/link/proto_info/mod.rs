// SPDX-License-Identifier: MIT

mod bridge;
mod inet6;

pub use self::bridge::LinkProtoInfoBridge;
pub use self::inet6::LinkProtoInfoInet6;

pub(crate) use self::bridge::VecLinkProtoInfoBridge;
pub(crate) use self::inet6::VecLinkProtoInfoInet6;
