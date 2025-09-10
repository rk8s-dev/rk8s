// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/net.h`

use crate::AF_MAX;

/// NET: An implementation of the SOCKET network access protocol.
/// This is the master header file for the Linux NET layer,
/// or, in plain English: the networking handling part of the kernel.
pub const NPROTO: i32 = AF_MAX;

// NOTE(Shaohua): Rename consts as they are used in sysno.rs
/// `sys_socket(2)`
pub const SYS_SOCKET_: i32 = 1;
/// `sys_bind(2)`
pub const SYS_BIND_: i32 = 2;
/// `sys_connect(2)`
pub const SYS_CONNECT_: i32 = 3;
/// `sys_listen(2)`
pub const SYS_LISTEN_: i32 = 4;
/// `sys_accept(2)`
pub const SYS_ACCEPT_: i32 = 5;
/// `sys_getsockname(2)`
pub const SYS_GETSOCKNAME_: i32 = 6;
/// `sys_getpeername(2)`
pub const SYS_GETPEERNAME_: i32 = 7;
/// `sys_socketpair(2)`
pub const SYS_SOCKETPAIR_: i32 = 8;
/// `sys_send(2)`
pub const SYS_SEND_: i32 = 9;
/// `sys_recv(2)`
pub const SYS_RECV_: i32 = 10;
/// `sys_sendto(2)`
pub const SYS_SENDTO_: i32 = 11;
/// `sys_recvfrom(2)`
pub const SYS_RECVFROM_: i32 = 12;
/// `sys_shutdown(2)`
pub const SYS_SHUTDOWN_: i32 = 13;
/// `sys_setsockopt(2)`
pub const SYS_SETSOCKOPT_: i32 = 14;
/// `sys_getsockopt(2)`
pub const SYS_GETSOCKOPT_: i32 = 15;
/// `sys_sendmsg(2)`
pub const SYS_SENDMSG_: i32 = 16;
/// `sys_recvmsg(2)`
pub const SYS_RECVMSG_: i32 = 17;
/// `sys_accept4(2)`
pub const SYS_ACCEPT4_: i32 = 18;
/// `sys_recvmmsg(2)`
pub const SYS_RECVMMSG_: i32 = 19;
/// `sys_sendmmsg(2)`
pub const SYS_SENDMMSG_: i32 = 20;

/// socket-state enum.
pub type socket_state_t = i32;
/// not allocated
pub const SS_FREE: socket_state_t = 0;
/// unconnected to any socket
pub const SS_UNCONNECTED: socket_state_t = 1;
/// in process of connecting
pub const SS_CONNECTING: socket_state_t = 2;
/// connected to socket
pub const SS_CONNECTED: socket_state_t = 3;
/// in process of disconnecting
pub const SS_DISCONNECTING: socket_state_t = 4;

/// performed a listen
pub const __SO_ACCEPTCON: i32 = 1 << 16;
