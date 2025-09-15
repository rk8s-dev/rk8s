// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/linux/net.h`
//!
//! NET: An implementation of the SOCKET network access protocol.
//! This is the master header file for the Linux NET layer,
//! or, in plain English: the networking handling part of the kernel.

use crate::{O_CLOEXEC, O_NONBLOCK};

/// Historically, `SOCKWQ_ASYNC_NOSPACE` & `SOCKWQ_ASYNC_WAITDATA` were located
/// in `sock->flags`, but moved into `sk->sk_wq->flags` to be RCU protected.
/// Eventually all flags will be in `sk->sk_wq->flags`.
pub const SOCKWQ_ASYNC_NOSPACE: i32 = 0;
pub const SOCKWQ_ASYNC_WAITDATA: i32 = 1;
pub const SOCK_NOSPACE: i32 = 2;
pub const SOCK_PASSCRED: i32 = 3;
pub const SOCK_PASSSEC: i32 = 4;

/// enum `sock_type` - Socket types
/// For writing rarp and other similar things on the user level.
///
/// When adding some new socket type please
/// grep `ARCH_HAS_SOCKET_TYPE` include/asm-* /socket.h, at least MIPS
/// overrides this enum for binary compat reasons.
///
/// Stream (connection) socket
pub const SOCK_STREAM: i32 = 1;
/// Datagram (conn.less) socket
pub const SOCK_DGRAM: i32 = 2;
/// Raw socket
pub const SOCK_RAW: i32 = 3;
/// Reliably-delivered message
pub const SOCK_RDM: i32 = 4;
/// sequential packet socket
pub const SOCK_SEQPACKET: i32 = 5;
/// Datagram Congestion Control Protocol socket
pub const SOCK_DCCP: i32 = 6;
/// Linux specific way of getting packets at the dev level.
pub const SOCK_PACKET: i32 = 10;

pub const SOCK_MAX: i32 = SOCK_PACKET + 1;

/// Mask which covers at least up to `SOCK_MASK - 1`.
/// The remaining bits are used as flags.
pub const SOCK_TYPE_MASK: i32 = 0xf;

/// Flags for socket, socketpair, accept4
pub const SOCK_CLOEXEC: i32 = O_CLOEXEC;

pub const SOCK_NONBLOCK: i32 = O_NONBLOCK;

/// enum `sock_shutdown_cmd` - Shutdown types:
///
/// Shutdown receptions
pub const SHUT_RD: i32 = 0;
/// Shutdown transmissions
pub const SHUT_WR: i32 = 1;
/// Shutdown receptions/transmissions
pub const SHUT_RDWR: i32 = 2;

//struct socket_wq {
//	// Note: wait MUST be first field of socket_wq
//	wait_queue_head_t	wait;
//
//	struct fasync_struct	*fasync_list;
//	unsigned long		flags; /* %SOCKWQ_ASYNC_NOSPACE, etc */
//	struct rcu_head		rcu;
//}

// struct socket - general BSD socket
// @state: socket state (%SS_CONNECTED, etc)
// @type: socket type (%SOCK_STREAM, etc)
// @flags: socket flags (%SOCK_NOSPACE, etc)
// @ops: protocol specific socket operations
// @file: File back pointer for gc
// @sk: internal networking protocol agnostic socket representation
// @wq: wait queue for several uses
//    #[repr(C)]
//pub struct socket_t {
//	pub state: socket_state_t,
//
//	pub type_: i16,
//
//	pub flags: usize,
//
//	struct file		*file;
//	struct sock		*sk;
//	const struct proto_ops	*ops;
//
//	struct socket_wq	wq;
//};

pub const SOCK_WAKE_IO: i32 = 0;
pub const SOCK_WAKE_WAITD: i32 = 1;
pub const SOCK_WAKE_SPACE: i32 = 2;
pub const SOCK_WAKE_URG: i32 = 3;
