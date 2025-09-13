// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/linux/socket.h`

use core::ffi::c_void;

use crate::{
    iovec_t, kernel_sa_family_t, kernel_sockaddr_storage_t, size_t, socklen_t, timespec64_t,
};

pub type sa_family_t = kernel_sa_family_t;

/// 1003.1g requires `sa_family_t` and that `sa_data` is char.
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct sockaddr_t {
    /// address family, `AF_xxx`
    pub sa_family: sa_family_t,
    /// 14 bytes of protocol address
    pub sa_data: [u8; 14],
}

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct linger_t {
    /// Linger active
    pub l_onoff: i32,
    /// How long to linger for
    pub l_linger: i32,
}

pub type sockaddr_storage_t = kernel_sockaddr_storage_t;

// As we do 4.4BSD message passing we use a 4.4BSD message passing
// system, not 4.3. Thus `msg_accrights(len)` are now missing. They
// belong in an obscure libc emulation or the bin.
//
// Comment kernel msg header
//#[repr(C)]
//struct msghdr_t {
//    /// ptr to socket address structure
//    pub msg_name: usize,
//    /// size of socket address structure
//    pub msg_namelen: i32,
//    /// data
//    //pub msg_iter: iov_iter_t,
//    pub msg_iter: usize,
//    /// ancillary data
//    pub msg_control: usize,
//    /// ancillary data buffer length
//    pub msg_controllen: size_t,
//    /// flags on received message
//    pub msg_flags: u32,
//    /// ptr to iocb for async requests
//    pub msg_iocb: *mut kiocb_t,
//}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct msghdr_t {
    /// ptr to socket address structure
    pub msg_name: *const c_void,
    /// size of socket address structure
    pub msg_namelen: socklen_t,
    /// scatter/gather array
    pub msg_iov: *mut iovec_t,
    /// # elements in `msg_iov`
    pub msg_iovlen: size_t,
    /// ancillary data
    pub msg_control: *const c_void,
    /// ancillary data buffer length
    pub msg_controllen: size_t,
    /// flags on received message
    pub msg_flags: i32,
}

pub type user_msghdr_t = msghdr_t;

/// For recvmmsg/sendmmsg
#[repr(C)]
#[derive(Debug, Clone)]
pub struct mmsghdr_t {
    pub msg_hdr: msghdr_t,
    pub msg_len: u32,
}

/// POSIX 1003.1g - ancillary data object information
/// Ancillary data consits of a sequence of pairs of
/// (`cmsghdr, cmsg_data[]`)
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct cmsghdr_t {
    /// data byte count, including hdr
    pub cmsg_len: size_t,
    /// originating protocol
    pub cmsg_level: i32,
    /// protocol-specific type
    pub cmsg_type: i32,
}

// Ancillary data object information MACROS
// Table 5-14 of POSIX 1003.1g
//
//#define __CMSG_NXTHDR(ctl, len, cmsg) __cmsg_nxthdr((ctl),(len),(cmsg))
//#define CMSG_NXTHDR(mhdr, cmsg) cmsg_nxthdr((mhdr), (cmsg))
//#define CMSG_ALIGN(len) ( ((len)+sizeof(long)-1) & ~(sizeof(long)-1) )
//#define CMSG_DATA(cmsg)	((void *)((char *)(cmsg) + sizeof(struct cmsghdr)))
//#define CMSG_SPACE(len) (sizeof(struct cmsghdr) + CMSG_ALIGN(len))
//#define CMSG_LEN(len) (sizeof(struct cmsghdr) + (len))
//#define __CMSG_FIRSTHDR(ctl,len) ((len) >= sizeof(struct cmsghdr) ? \
//				  (struct cmsghdr *)(ctl) : \
//				  (struct cmsghdr *)NULL)
//#define CMSG_FIRSTHDR(msg)	__CMSG_FIRSTHDR((msg)->msg_control, (msg)->msg_controllen)
//#define CMSG_OK(mhdr, cmsg) ((cmsg)->cmsg_len >= sizeof(struct cmsghdr) && \
//			     (cmsg)->cmsg_len <= (unsigned long) \
//			     ((mhdr)->msg_controllen - \
//			      ((char *)(cmsg) - (char *)(mhdr)->msg_control)))
//#define for_each_cmsghdr(cmsg, msg) \
//	for (cmsg = CMSG_FIRSTHDR(msg); \
//	     cmsg; \
//	     cmsg = CMSG_NXTHDR(msg, cmsg))

/// "Socket"-level control message types:
///
/// rw: access rights (array of int)
pub const SCM_RIGHTS: i32 = 0x01;
/// rw: struct ucred
pub const SCM_CREDENTIALS: i32 = 0x02;
/// rw: security label
pub const SCM_SECURITY: i32 = 0x03;

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct ucred_t {
    pub pid: u32,
    pub uid: u32,
    pub gid: u32,
}

/// Supported address families.
pub const AF_UNSPEC: i32 = 0;
/// Unix domain sockets
pub const AF_UNIX: i32 = 1;
/// POSIX name for `AF_UNIX`
pub const AF_LOCAL: i32 = 1;
/// Internet IP Protocol
pub const AF_INET: i32 = 2;
/// Amateur Radio AX.25
pub const AF_AX25: i32 = 3;
/// Novell IPX
pub const AF_IPX: i32 = 4;
/// `AppleTalk` DDP
pub const AF_APPLETALK: i32 = 5;
/// Amateur Radio NET/ROM
pub const AF_NETROM: i32 = 6;
/// Multiprotocol bridge
pub const AF_BRIDGE: i32 = 7;
/// ATM PVCs
pub const AF_ATMPVC: i32 = 8;
/// Reserved for X.25 project
pub const AF_X25: i32 = 9;
/// IP version 6
pub const AF_INET6: i32 = 10;
/// Amateur Radio X.25 PLP
pub const AF_ROSE: i32 = 11;
/// Reserved for `DECnet` project
pub const AF_DECNET: i32 = 12;
/// Reserved for 802.2LLC project
pub const AF_NETBEUI: i32 = 13;
/// Security callback pseudo AF
pub const AF_SECURITY: i32 = 14;
/// `PF_KEY` key management API
pub const AF_KEY: i32 = 15;
pub const AF_NETLINK: i32 = 16;
/// Alias to emulate 4.4BSD
pub const AF_ROUTE: i32 = AF_NETLINK;
/// Packet family
pub const AF_PACKET: i32 = 17;
/// Ash
pub const AF_ASH: i32 = 18;
/// Acorn Econet
pub const AF_ECONET: i32 = 19;
/// ATM SVCs
pub const AF_ATMSVC: i32 = 20;
/// RDS sockets
pub const AF_RDS: i32 = 21;
/// Linux SNA Project (nutters!)
pub const AF_SNA: i32 = 22;
/// IRDA sockets
pub const AF_IRDA: i32 = 23;
/// `PPPoX` sockets
pub const AF_PPPOX: i32 = 24;
/// Wanpipe API Sockets
pub const AF_WANPIPE: i32 = 25;
/// Linux LLC
pub const AF_LLC: i32 = 26;
/// `Native InfiniBand` address
pub const AF_IB: i32 = 27;
/// MPLS
pub const AF_MPLS: i32 = 28;
/// Controller Area Network
pub const AF_CAN: i32 = 29;
/// TIPC sockets
pub const AF_TIPC: i32 = 30;
/// Bluetooth sockets
pub const AF_BLUETOOTH: i32 = 31;
/// IUCV sockets
pub const AF_IUCV: i32 = 32;
/// `RxRPC` sockets
pub const AF_RXRPC: i32 = 33;
/// `mISDN` sockets
pub const AF_ISDN: i32 = 34;
/// Phonet sockets
pub const AF_PHONET: i32 = 35;
/// IEEE802154 sockets
pub const AF_IEEE802154: i32 = 36;
/// CAIF sockets
pub const AF_CAIF: i32 = 37;
/// Algorithm sockets
pub const AF_ALG: i32 = 38;
/// NFC sockets
pub const AF_NFC: i32 = 39;
/// vSockets
pub const AF_VSOCK: i32 = 40;
/// Kernel Connection Multiplexor
pub const AF_KCM: i32 = 41;
/// Qualcomm IPC Router
pub const AF_QIPCRTR: i32 = 42;
/// smc sockets: reserve number for `PF_SMC` protocol family that reuses
/// `AF_INET` address family
pub const AF_SMC: i32 = 43;
/// XDP sockets
pub const AF_XDP: i32 = 44;

/// For now..
pub const AF_MAX: i32 = 45;

/// Protocol families, same as address families.
pub const PF_UNSPEC: i32 = AF_UNSPEC;
pub const PF_UNIX: i32 = AF_UNIX;
pub const PF_LOCAL: i32 = AF_LOCAL;
pub const PF_INET: i32 = AF_INET;
pub const PF_AX25: i32 = AF_AX25;
pub const PF_IPX: i32 = AF_IPX;
pub const PF_APPLETALK: i32 = AF_APPLETALK;
pub const PF_NETROM: i32 = AF_NETROM;
pub const PF_BRIDGE: i32 = AF_BRIDGE;
pub const PF_ATMPVC: i32 = AF_ATMPVC;
pub const PF_X25: i32 = AF_X25;
pub const PF_INET6: i32 = AF_INET6;
pub const PF_ROSE: i32 = AF_ROSE;
pub const PF_DECNET: i32 = AF_DECNET;
pub const PF_NETBEUI: i32 = AF_NETBEUI;
pub const PF_SECURITY: i32 = AF_SECURITY;
pub const PF_KEY: i32 = AF_KEY;
pub const PF_NETLINK: i32 = AF_NETLINK;
pub const PF_ROUTE: i32 = AF_ROUTE;
pub const PF_PACKET: i32 = AF_PACKET;
pub const PF_ASH: i32 = AF_ASH;
pub const PF_ECONET: i32 = AF_ECONET;
pub const PF_ATMSVC: i32 = AF_ATMSVC;
pub const PF_RDS: i32 = AF_RDS;
pub const PF_SNA: i32 = AF_SNA;
pub const PF_IRDA: i32 = AF_IRDA;
pub const PF_PPPOX: i32 = AF_PPPOX;
pub const PF_WANPIPE: i32 = AF_WANPIPE;
pub const PF_LLC: i32 = AF_LLC;
pub const PF_IB: i32 = AF_IB;
pub const PF_MPLS: i32 = AF_MPLS;
pub const PF_CAN: i32 = AF_CAN;
pub const PF_TIPC: i32 = AF_TIPC;
pub const PF_BLUETOOTH: i32 = AF_BLUETOOTH;
pub const PF_IUCV: i32 = AF_IUCV;
pub const PF_RXRPC: i32 = AF_RXRPC;
pub const PF_ISDN: i32 = AF_ISDN;
pub const PF_PHONET: i32 = AF_PHONET;
pub const PF_IEEE802154: i32 = AF_IEEE802154;
pub const PF_CAIF: i32 = AF_CAIF;
pub const PF_ALG: i32 = AF_ALG;
pub const PF_NFC: i32 = AF_NFC;
pub const PF_VSOCK: i32 = AF_VSOCK;
pub const PF_KCM: i32 = AF_KCM;
pub const PF_QIPCRTR: i32 = AF_QIPCRTR;
pub const PF_SMC: i32 = AF_SMC;
pub const PF_XDP: i32 = AF_XDP;
pub const PF_MAX: i32 = AF_MAX;

/// Maximum queue length specifiable by listen.
pub const SOMAXCONN: i32 = 128;

/// Flags we can use with send/ and recv.
/// Added those for 1003.1g not all are supported yet
pub const MSG_OOB: i32 = 1;
pub const MSG_PEEK: i32 = 2;
pub const MSG_DONTROUTE: i32 = 4;
/// Synonym for `MSG_DONTROUTE` for `DECnet`
pub const MSG_TRYHARD: i32 = 4;
pub const MSG_CTRUNC: i32 = 8;
/// Do not send. Only probe path f.e. for MTU
pub const MSG_PROBE: i32 = 0x10;
pub const MSG_TRUNC: i32 = 0x20;
/// Nonblocking io
pub const MSG_DONTWAIT: i32 = 0x40;
/// End of record
pub const MSG_EOR: i32 = 0x80;
/// Wait for a full request
pub const MSG_WAITALL: i32 = 0x100;
pub const MSG_FIN: i32 = 0x200;
pub const MSG_SYN: i32 = 0x400;
/// Confirm path validity
pub const MSG_CONFIRM: i32 = 0x800;
pub const MSG_RST: i32 = 0x1000;
/// Fetch message from error queue
pub const MSG_ERRQUEUE: i32 = 0x2000;
/// Do not generate SIGPIPE
pub const MSG_NOSIGNAL: i32 = 0x4000;
/// Sender will send more
pub const MSG_MORE: i32 = 0x8000;
/// `recvmmsg()`: block until 1+ packets avail
pub const MSG_WAITFORONE: i32 = 0x10000;
/// `sendpage()` internal : do no apply policy
pub const MSG_SENDPAGE_NOPOLICY: i32 = 0x10000;
/// `sendpage()` internal : not the last page
pub const MSG_SENDPAGE_NOTLAST: i32 = 0x20000;
/// `sendmmsg()`: more messages coming
pub const MSG_BATCH: i32 = 0x40000;
pub const MSG_EOF: i32 = MSG_FIN;
/// `sendpage()` internal : page frags are not shared
pub const MSG_NO_SHARED_FRAGS: i32 = 0x80000;
/// `sendpage()` internal : page may carry plain text and require encryption
pub const MSG_SENDPAGE_DECRYPTED: i32 = 0x0010_0000;

/// Use user data in kernel path
pub const MSG_ZEROCOPY: i32 = 0x0400_0000;
/// Send data in TCP SYN
pub const MSG_FASTOPEN: i32 = 0x2000_0000;
/// Set `close_on_exec` for file descriptor received through `SCM_RIGHTS`
pub const MSG_CMSG_CLOEXEC: i32 = 0x4000_0000;

// TODO(Shaohua): Support compat
//#if defined(CONFIG_COMPAT)
// This message needs 32 bit fixups
//pub const MSG_CMSG_COMPAT: i32 = 0x80000000;
/// We never have 32 bit fixups
pub const MSG_CMSG_COMPAT: i32 = 0;

/// `setsockoptions(2)` level. Thanks to BSD these must match `IPPROTO_xxx`
pub const SOL_IP: i32 = 0;
/// #define `SOL_ICMP` 1 No-no-no! Due to Linux :-) we cannot use `SOL_ICMP=1`
pub const SOL_TCP: i32 = 6;
pub const SOL_UDP: i32 = 17;
pub const SOL_IPV6: i32 = 41;
pub const SOL_ICMPV6: i32 = 58;
pub const SOL_SCTP: i32 = 132;
/// UDP-Lite (RFC 3828)
pub const SOL_UDPLITE: i32 = 136;
pub const SOL_RAW: i32 = 255;
pub const SOL_IPX: i32 = 256;
pub const SOL_AX25: i32 = 257;
pub const SOL_ATALK: i32 = 258;
pub const SOL_NETROM: i32 = 259;
pub const SOL_ROSE: i32 = 260;
pub const SOL_DECNET: i32 = 261;
pub const SOL_X25: i32 = 262;
pub const SOL_PACKET: i32 = 263;
/// ATM layer (cell level)
pub const SOL_ATM: i32 = 264;
/// ATM Adaption Layer (packet level)
pub const SOL_AAL: i32 = 265;
pub const SOL_IRDA: i32 = 266;
pub const SOL_NETBEUI: i32 = 267;
pub const SOL_LLC: i32 = 268;
pub const SOL_DCCP: i32 = 269;
pub const SOL_NETLINK: i32 = 270;
pub const SOL_TIPC: i32 = 271;
pub const SOL_RXRPC: i32 = 272;
pub const SOL_PPPOL2TP: i32 = 273;
pub const SOL_BLUETOOTH: i32 = 274;
pub const SOL_PNPIPE: i32 = 275;
pub const SOL_RDS: i32 = 276;
pub const SOL_IUCV: i32 = 277;
pub const SOL_CAIF: i32 = 278;
pub const SOL_ALG: i32 = 279;
pub const SOL_NFC: i32 = 280;
pub const SOL_KCM: i32 = 281;
pub const SOL_TLS: i32 = 282;
pub const SOL_XDP: i32 = 283;

/// IPX options
pub const IPX_TYPE: i32 = 1;

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct scm_timestamping_internal_t {
    pub ts: [timespec64_t; 3],
}
