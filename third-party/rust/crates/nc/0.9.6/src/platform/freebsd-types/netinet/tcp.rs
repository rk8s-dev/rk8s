// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `netinet/tcp.h`

#![allow(clippy::module_name_repetitions)]

use core::mem::size_of;

use crate::SO_VENDOR;

pub type tcp_seq = u32;

/// for KAME src sync over BSD*'s
pub type tcp6_seq = tcp_seq;
/// for KAME src sync over BSD*'s
pub type tcp6hdr_t = tcphdr_t;

pub const TH_FIN: i32 = 0x01;
pub const TH_SYN: i32 = 0x02;
pub const TH_RST: i32 = 0x04;
pub const TH_PUSH: i32 = 0x08;
pub const TH_ACK: i32 = 0x10;
pub const TH_URG: i32 = 0x20;
pub const TH_ECE: i32 = 0x40;
pub const TH_CWR: i32 = 0x80;
/// maps into `th_x2`
pub const TH_AE: i32 = 0x100;
pub const TH_FLAGS: i32 = TH_FIN | TH_SYN | TH_RST | TH_PUSH | TH_ACK | TH_URG | TH_ECE | TH_CWR;

/// TCP header.
/// Per RFC 793, September, 1981.
#[repr(C)]
pub struct tcphdr_t {
    /// source port
    pub th_sport: u16,
    /// destination port
    pub th_dport: u16,
    /// sequence number
    pub th_seq: tcp_seq,
    /// acknowledgement number
    pub th_ack: tcp_seq,

    // (unused)
    //pub th_x2: 4,
    #[cfg(target_endian = "little")]
    /// data offset
    pub th_off: u8,

    #[cfg(target_endian = "big")]
    /// data offset
    pub th_off: u8,
    // (unused)
    //pub th_x2: 4,
    pub th_flags: u8,
    /// window
    pub th_win: u16,
    /// checksum
    pub th_sum: u16,
    /// urgent pointer
    pub th_urp: u16,
}

pub const TCPOPT_EOL: i32 = 0;
pub const TCPOLEN_EOL: i32 = 1;
/// padding after EOL
pub const TCPOPT_PAD: i32 = 0;
pub const TCPOLEN_PAD: i32 = 1;
pub const TCPOPT_NOP: i32 = 1;
pub const TCPOLEN_NOP: i32 = 1;
pub const TCPOPT_MAXSEG: i32 = 2;
pub const TCPOLEN_MAXSEG: i32 = 4;
pub const TCPOPT_WINDOW: i32 = 3;
pub const TCPOLEN_WINDOW: i32 = 3;
pub const TCPOPT_SACK_PERMITTED: i32 = 4;
pub const TCPOLEN_SACK_PERMITTED: i32 = 2;
pub const TCPOPT_SACK: i32 = 5;
pub const TCPOLEN_SACKHDR: i32 = 2;
/// `2 * sizeof(tcp_seq)`
pub const TCPOLEN_SACK: i32 = 8;
pub const TCPOPT_TIMESTAMP: i32 = 8;
pub const TCPOLEN_TIMESTAMP: i32 = 10;
/// appendix A
pub const TCPOLEN_TSTAMP_APPA: i32 = TCPOLEN_TIMESTAMP + 2;
/// Keyed MD5: RFC 2385
pub const TCPOPT_SIGNATURE: i32 = 19;
pub const TCPOLEN_SIGNATURE: i32 = 18;
pub const TCPOPT_FAST_OPEN: i32 = 34;
pub const TCPOLEN_FAST_OPEN_EMPTY: i32 = 2;

/// Absolute maximum TCP options len
pub const MAX_TCPOPTLEN: i32 = 40;

/// Miscellaneous constants
/// Max # SACK blocks stored at receiver side
pub const MAX_SACK_BLKS: i32 = 6;
/// MAX # SACKs sent in any segment
pub const TCP_MAX_SACK: i32 = 4;

/// The default maximum segment size (MSS) to be used for new TCP connections
/// when path MTU discovery is not enabled.
///
/// RFC879 derives the default MSS from the largest datagram size hosts are
/// minimally required to handle directly or through IP reassembly minus the
/// size of the IP and TCP header.  With IPv6 the minimum MTU is specified
/// in RFC2460.
///
/// For IPv4 the MSS is `576 - sizeof(struct tcpiphdr)`
/// For IPv6 the MSS is `IPV6_MMTU - sizeof(struct ip6_hdr) - sizeof(struct tcphdr)`
///
/// We use explicit numerical definition here to avoid header pollution.
pub const TCP_MSS: i32 = 536;
pub const TCP6_MSS: i32 = 1220;

/// Limit the lowest MSS we accept for path MTU discovery and the TCP SYN MSS
/// option.  Allowing low values of MSS can consume significant resources and
/// be used to mount a resource exhaustion attack.
/// Connections requesting lower MSS values will be rounded up to this value
/// and the `IP_DF` flag will be cleared to allow fragmentation along the path.
///
/// See `tcp_subr.c` `tcp_minmss SYSCTL` declaration for more comments.  Setting
/// it to "0" disables the minmss check.
///
/// The default value is fine for TCP across the Internet's smallest official
/// link MTU (256 bytes for AX.25 packet radio).  However, a connection is very
/// unlikely to come across such low MTU interfaces these days (anno domini 2003).
pub const TCP_MINMSS: i32 = 216;

/// largest value for (unscaled) window
pub const TCP_MAXWIN: i32 = 65535;
/// dflt send window for T/TCP client
pub const TTCP_CLIENT_SND_WND: i32 = 4096;

/// maximum window shift
pub const TCP_MAX_WINSHIFT: i32 = 14;

/// maximum segments in a burst
pub const TCP_MAXBURST: i32 = 4;

/// max length of header in bytes
pub const TCP_MAXHLEN: usize = 0xf << 2;
/// max space left for options
pub const TCP_MAXOLEN: usize = TCP_MAXHLEN - size_of::<tcphdr_t>();

/// Per RFC7413
pub const TCP_FASTOPEN_MIN_COOKIE_LEN: usize = 4;
/// Per RFC7413
pub const TCP_FASTOPEN_MAX_COOKIE_LEN: usize = 16;
/// Same as `TCP_FASTOPEN_KEY_LEN`
pub const TCP_FASTOPEN_PSK_LEN: usize = 16;

/// User-settable options (used with setsockopt).  These are discrete
/// values and are not masked together.  Some values appear to be
/// bitmasks for historical reasons.
/// don't delay send to coalesce packets
pub const TCP_NODELAY: i32 = 1;
/// set maximum segment size
pub const TCP_MAXSEG: i32 = 2;
/// don't push last block of write
pub const TCP_NOPUSH: i32 = 4;
/// don't use TCP options
pub const TCP_NOOPT: i32 = 8;
/// use MD5 digests (RFC2385)
pub const TCP_MD5SIG: i32 = 16;
/// retrieve `tcp_info` structure
pub const TCP_INFO: i32 = 32;
/// retrieve stats blob structure
pub const TCP_STATS: i32 = 33;
/// configure event logging for connection
pub const TCP_LOG: i32 = 34;
/// retrieve event log for connection
pub const TCP_LOGBUF: i32 = 35;
/// configure log ID to correlate connections
pub const TCP_LOGID: i32 = 36;
/// dump connection log events to device
pub const TCP_LOGDUMP: i32 = 37;
/// dump events from connections with same ID to device
pub const TCP_LOGDUMPID: i32 = 38;
/// TLS framing and encryption for transmit
pub const TCP_TXTLS_ENABLE: i32 = 39;
/// Transmit TLS mode
pub const TCP_TXTLS_MODE: i32 = 40;
/// TLS framing and encryption for receive
pub const TCP_RXTLS_ENABLE: i32 = 41;
/// Receive TLS mode
pub const TCP_RXTLS_MODE: i32 = 42;
/// Override initial window (units: bytes)
pub const TCP_IWND_NB: i32 = 43;
/// Override initial window (units: MSS segs)
pub const TCP_IWND_NSEG: i32 = 44;
/// get number of connections with the same ID
pub const TCP_LOGID_CNT: i32 = 46;
/// configure tag for grouping logs
pub const TCP_LOG_TAG: i32 = 47;
/// userspace log event
pub const TCP_USER_LOG: i32 = 48;
/// get/set congestion control algorithm
pub const TCP_CONGESTION: i32 = 64;
/// get/set cc algorithm specific options
pub const TCP_CCALGOOPT: i32 = 65;
/// maximum time without making progress (sec)
pub const TCP_MAXUNACKTIME: i32 = 68;
/// maximum peak rate allowed (kbps)
pub const TCP_MAXPEAKRATE: i32 = 69;
/// Reduce cwnd on idle input
pub const TCP_IDLE_REDUCE: i32 = 70;
/// Enable TCP over UDP tunneling via the specified port
pub const TCP_REMOTE_UDP_ENCAPS_PORT: i32 = 71;
/// socket option for delayed ack
pub const TCP_DELACK: i32 = 72;
/// A fin from the peer is treated has a RST
pub const TCP_FIN_IS_RST: i32 = 73;
/// Limit to number of records in tcp-log
pub const TCP_LOG_LIMIT: i32 = 74;
/// Use of a shared cwnd is allowed
pub const TCP_SHARED_CWND_ALLOWED: i32 = 75;
/// Do accounting on tcp cpu usage and counts
pub const TCP_PROC_ACCOUNTING: i32 = 76;
/// The transport can handle the Compressed mbuf acks
pub const TCP_USE_CMP_ACKS: i32 = 77;
/// retrieve accounting counters
pub const TCP_PERF_INFO: i32 = 78;
/// N, time to establish connection
pub const TCP_KEEPINIT: i32 = 128;
/// L,N,X start keeplives after this period
pub const TCP_KEEPIDLE: i32 = 256;
/// L,N interval between keepalives
pub const TCP_KEEPINTVL: i32 = 512;
/// L,N number of keepalives before close
pub const TCP_KEEPCNT: i32 = 1024;
/// enable TFO / was created via TFO
pub const TCP_FASTOPEN: i32 = 1025;
/// number of output packets to keep
pub const TCP_PCAP_OUT: i32 = 2048;
/// number of input packets to keep
pub const TCP_PCAP_IN: i32 = 4096;
/// Set the tcp function pointers to the specified stack
pub const TCP_FUNCTION_BLK: i32 = 8192;
/// Options for Rack and BBR
/// set listen socket numa domain
pub const TCP_REUSPORT_LB_NUMA: i32 = 1026;
/// Do we allow mbuf queuing if supported
pub const TCP_RACK_MBUF_QUEUE: i32 = 1050;
/// RACK proportional rate reduction (bool)
pub const TCP_RACK_PROP: i32 = 1051;
/// RACK TLP cwnd reduction (bool)
pub const TCP_RACK_TLP_REDUCE: i32 = 1052;
/// RACK Pacingv reduction factor (divisor)
pub const TCP_RACK_PACE_REDUCE: i32 = 1053;
/// Max TSO size we will send
pub const TCP_RACK_PACE_MAX_SEG: i32 = 1054;
/// Use the always pace method
pub const TCP_RACK_PACE_ALWAYS: i32 = 1055;
/// The proportional reduction rate
pub const TCP_RACK_PROP_RATE: i32 = 1056;
/// Allow PRR to send more than one seg
pub const TCP_RACK_PRR_SENDALOT: i32 = 1057;
/// Minimum time between rack t-o's in ms
pub const TCP_RACK_MIN_TO: i32 = 1058;
/// Should recovery happen early (bool)
pub const TCP_RACK_EARLY_RECOV: i32 = 1059;
/// If early recovery max segments
pub const TCP_RACK_EARLY_SEG: i32 = 1060;
/// RACK reorder threshold (shift amount)
pub const TCP_RACK_REORD_THRESH: i32 = 1061;
/// Does reordering fade after ms time
pub const TCP_RACK_REORD_FADE: i32 = 1062;
/// RACK TLP theshold i.e. srtt+(srtt/N)
pub const TCP_RACK_TLP_THRESH: i32 = 1063;
/// RACK added ms i.e. rack-rtt + reord + N
pub const TCP_RACK_PKT_DELAY: i32 = 1064;
/// Does TLP include rtt variance in t-o
pub const TCP_RACK_TLP_INC_VAR: i32 = 1065;
/// Initial TSO window for BBRs first sends
pub const TCP_BBR_IWINTSO: i32 = 1067;
/// Enter recovery force out a segment disregard pacer no longer valid
pub const TCP_BBR_RECFORCE: i32 = 1068;
/// Startup pacing gain
pub const TCP_BBR_STARTUP_PG: i32 = 1069;
/// Drain pacing gain
pub const TCP_BBR_DRAIN_PG: i32 = 1070;
/// Rwnd limited is considered app limited
pub const TCP_BBR_RWND_IS_APP: i32 = 1071;
/// How long in useconds between probe-rtt
pub const TCP_BBR_PROBE_RTT_INT: i32 = 1072;
/// Is only one segment allowed out during retran
pub const TCP_BBR_ONE_RETRAN: i32 = 1073;
/// Do we exit a loss during startup if not 20% incr
pub const TCP_BBR_STARTUP_LOSS_EXIT: i32 = 1074;
/// lower the gain in `PROBE_BW` enable
pub const TCP_BBR_USE_LOWGAIN: i32 = 1075;
/// Unused after 2.3 morphs to TSLIMITS >= 2.3
pub const TCP_BBR_LOWGAIN_THRESH: i32 = 1076;
/// Do we use experimental Timestamp limiting for our algo
pub const TCP_BBR_TSLIMITS: i32 = 1076;
/// Unused after 2.3
pub const TCP_BBR_LOWGAIN_HALF: i32 = 1077;
/// Reused in 4.2 for pacing overhead setting
pub const TCP_BBR_PACE_OH: i32 = 1077;
/// Unused after 2.3
pub const TCP_BBR_LOWGAIN_FD: i32 = 1078;
/// For 4.3 on
pub const TCP_BBR_HOLD_TARGET: i32 = 1078;
/// Enable use of delivery rate for loss recovery
pub const TCP_BBR_USEDEL_RATE: i32 = 1079;
/// Min RTO in milliseconds
pub const TCP_BBR_MIN_RTO: i32 = 1080;
/// Max RTO in milliseconds
pub const TCP_BBR_MAX_RTO: i32 = 1081;
/// Recovery override htps settings 0/1/3
pub const TCP_BBR_REC_OVER_HPTS: i32 = 1082;
/// Not used before 2.3 and morphs to algorithm >= 2.3
pub const TCP_BBR_UNLIMITED: i32 = 1083;
/// What measurement algo does BBR use netflix=0, google=1
pub const TCP_BBR_ALGORITHM: i32 = 1083;
/// Does the 3/4 drain target include the extra gain
pub const TCP_BBR_DRAIN_INC_EXTRA: i32 = 1084;
/// what epoch gets us out of startup
pub const TCP_BBR_STARTUP_EXIT_EPOCH: i32 = 1085;
pub const TCP_BBR_PACE_PER_SEC: i32 = 1086;
pub const TCP_BBR_PACE_DEL_TAR: i32 = 1087;
pub const TCP_BBR_PACE_SEG_MAX: i32 = 1088;
pub const TCP_BBR_PACE_SEG_MIN: i32 = 1089;
pub const TCP_BBR_PACE_CROSS: i32 = 1090;
/// Reduce the highest cwnd seen to IW on idle
pub const TCP_RACK_IDLE_REDUCE_HIGH: i32 = 1092;
/// Do we enforce rack min pace time
pub const TCP_RACK_MIN_PACE: i32 = 1093;
/// If so what is the seg threshould
pub const TCP_RACK_MIN_PACE_SEG: i32 = 1094;
/// After 4.1 its the GP increase in older rack
pub const TCP_RACK_GP_INCREASE: i32 = 1094;
pub const TCP_RACK_TLP_USE: i32 = 1095;
/// Not used
pub const TCP_BBR_ACK_COMP_ALG: i32 = 1096;
/// Recycled in 4.2
pub const TCP_BBR_TMR_PACE_OH: i32 = 1096;
pub const TCP_BBR_EXTRA_GAIN: i32 = 1097;
/// Recycle of extra gain for rack, attack detection
pub const TCP_RACK_DO_DETECTION: i32 = 1097;
/// what RTT should we use 0, 1, or 2?
pub const TCP_BBR_RACK_RTT_USE: i32 = 1098;
pub const TCP_BBR_RETRAN_WTSO: i32 = 1099;
pub const TCP_DATA_AFTER_CLOSE: i32 = 1100;
pub const TCP_BBR_PROBE_RTT_GAIN: i32 = 1101;
pub const TCP_BBR_PROBE_RTT_LEN: i32 = 1102;
/// Do we burst out whole iwin size chunks at start?
pub const TCP_BBR_SEND_IWND_IN_TSO: i32 = 1103;
/// Do we use the rack rapid recovery for pacing rxt's
pub const TCP_BBR_USE_RACK_RR: i32 = 1104;
/// Compat.
pub const TCP_BBR_USE_RACK_CHEAT: i32 = TCP_BBR_USE_RACK_RR;
/// Enable/disable hardware pacing
pub const TCP_BBR_HDWR_PACE: i32 = 1105;
/// Do we enforce an utter max TSO size
pub const TCP_BBR_UTTER_MAX_TSO: i32 = 1106;
/// Special exit-persist catch up
pub const TCP_BBR_EXTRA_STATE: i32 = 1107;
/// The min tso size
pub const TCP_BBR_FLOOR_MIN_TSO: i32 = 1108;
/// Do we suspend pacing until
pub const TCP_BBR_MIN_TOPACEOUT: i32 = 1109;
/// Can a timestamp measurement raise the b/w
pub const TCP_BBR_TSTMP_RAISES: i32 = 1110;
/// Turn on/off google mode policer detection
pub const TCP_BBR_POLICER_DETECT: i32 = 1111;
/// Set an initial pacing rate for when we have no b/w in kbits per sec
pub const TCP_BBR_RACK_INIT_RATE: i32 = 1112;
/// Rack rapid recovery configuration control
pub const TCP_RACK_RR_CONF: i32 = 1113;
pub const TCP_RACK_CHEAT_NOT_CONF_RATE: i32 = TCP_RACK_RR_CONF;
/// GP increase for Congestion Avoidance
pub const TCP_RACK_GP_INCREASE_CA: i32 = 1114;
/// GP increase for Slow Start
pub const TCP_RACK_GP_INCREASE_SS: i32 = 1115;
/// GP increase for Recovery
pub const TCP_RACK_GP_INCREASE_REC: i32 = 1116;
/// Override to use the user set max-seg value
pub const TCP_RACK_FORCE_MSEG: i32 = 1117;
/// Pacing rate for Congestion Avoidance
pub const TCP_RACK_PACE_RATE_CA: i32 = 1118;
/// Pacing rate for Slow Start
pub const TCP_RACK_PACE_RATE_SS: i32 = 1119;
/// Pacing rate for Recovery
pub const TCP_RACK_PACE_RATE_REC: i32 = 1120;
/// If pacing, don't use prr
pub const TCP_NO_PRR: i32 = 1122;
/// In recovery does a non-rxt use the cfg rate
pub const TCP_RACK_NONRXT_CFG_RATE: i32 = 1123;
/// Use a shared cwnd if allowed
pub const TCP_SHARED_CWND_ENABLE: i32 = 1124;
/// Do we attempt dynamic multipler adjustment with timely.
pub const TCP_TIMELY_DYN_ADJ: i32 = 1125;
/// For timely do not push if we are over max rtt
pub const TCP_RACK_NO_PUSH_AT_MAX: i32 = 1126;
/// If we are not in recovery, always pace to fill the cwnd in 1 RTT
pub const TCP_RACK_PACE_TO_FILL: i32 = 1127;
/// we should limit to low time values the scwnd life
pub const TCP_SHARED_CWND_TIME_LIMIT: i32 = 1128;
/// Select a profile that sets multiple options
pub const TCP_RACK_PROFILE: i32 = 1129;
/// Allow hardware rates to cap pacing rate
pub const TCP_HDWR_RATE_CAP: i32 = 1130;
/// Highest rate allowed in pacing in bytes per second (`uint64_t`)
pub const TCP_PACING_RATE_CAP: i32 = 1131;
/// Allow the pacing rate to climb but not descend (with the exception of fill-cw)
pub const TCP_HDWR_UP_ONLY: i32 = 1132;
/// Set a local ABC value different then the system default
pub const TCP_RACK_ABC_VAL: i32 = 1133;
/// Do we use the ABC value for recovery or the override one from sysctl
pub const TCP_REC_ABC_VAL: i32 = 1134;
/// How many measurements are required in GP pacing
pub const TCP_RACK_MEASURE_CNT: i32 = 1135;
/// Defer options until the proper number of measurements occur, does not defer `TCP_RACK_MEASURE_CNT`
pub const TCP_DEFER_OPTIONS: i32 = 1136;
/// Do we do the broken thing where we don't twiddle the TLP bits properly in `fast_rsm_output`?
pub const TCP_FAST_RSM_HACK: i32 = 1137;
/// Changing the beta for pacing
pub const TCP_RACK_PACING_BETA: i32 = 1138;
/// Changing the beta for ecn with pacing
pub const TCP_RACK_PACING_BETA_ECN: i32 = 1139;
/// Set or get the timer slop used
pub const TCP_RACK_TIMER_SLOP: i32 = 1140;

/// Start of reserved space for third-party user-settable options.
pub const TCP_VENDOR: i32 = SO_VENDOR;

/// max congestion control name length
pub const TCP_CA_NAME_MAX: i32 = 16;

pub const TCPI_OPT_TIMESTAMPS: i32 = 0x01;
pub const TCPI_OPT_SACK: i32 = 0x02;
pub const TCPI_OPT_WSCALE: i32 = 0x04;
pub const TCPI_OPT_ECN: i32 = 0x08;
pub const TCPI_OPT_TOE: i32 = 0x10;
pub const TCPI_OPT_TFO: i32 = 0x20;

/// Maximum length of log ID.
pub const TCP_LOG_ID_LEN: i32 = 64;

///*
// * The TCP_INFO socket option comes from the Linux 2.6 TCP API, and permits
// * the caller to query certain information about the state of a TCP
// * connection.  We provide an overlapping set of fields with the Linux
// * implementation, but since this is a fixed size structure, room has been
// * left for growth.  In order to maximize potential future compatibility with
// * the Linux API, the same variable names and order have been adopted, and
// * padding left to make room for omitted fields in case they are added later.
// *
// * XXX: This is currently an unstable ABI/API, in that it is expected to
// * change.
// */
//pub struct tcp_info_t {
//	u_int8_t	tcpi_state;		/* TCP FSM state. */
//	u_int8_t	__tcpi_ca_state;
//	u_int8_t	__tcpi_retransmits;
//	u_int8_t	__tcpi_probes;
//	u_int8_t	__tcpi_backoff;
//	u_int8_t	tcpi_options;		/* Options enabled on conn. */
//	u_int8_t	tcpi_snd_wscale:4,	/* RFC1323 send shift value. */
//			tcpi_rcv_wscale:4;	/* RFC1323 recv shift value. */
//
//	u_int32_t	tcpi_rto;		/* Retransmission timeout (usec). */
//	u_int32_t	__tcpi_ato;
//	u_int32_t	tcpi_snd_mss;		/* Max segment size for send. */
//	u_int32_t	tcpi_rcv_mss;		/* Max segment size for receive. */
//
//	u_int32_t	__tcpi_unacked;
//	u_int32_t	__tcpi_sacked;
//	u_int32_t	__tcpi_lost;
//	u_int32_t	__tcpi_retrans;
//	u_int32_t	__tcpi_fackets;
//
//	/* Times; measurements in usecs. */
//	u_int32_t	__tcpi_last_data_sent;
//	u_int32_t	__tcpi_last_ack_sent;	/* Also unimpl. on Linux? */
//	u_int32_t	tcpi_last_data_recv;	/* Time since last recv data. */
//	u_int32_t	__tcpi_last_ack_recv;
//
//	/* Metrics; variable units. */
//	u_int32_t	__tcpi_pmtu;
//	u_int32_t	__tcpi_rcv_ssthresh;
//	u_int32_t	tcpi_rtt;		/* Smoothed RTT in usecs. */
//	u_int32_t	tcpi_rttvar;		/* RTT variance in usecs. */
//	u_int32_t	tcpi_snd_ssthresh;	/* Slow start threshold. */
//	u_int32_t	tcpi_snd_cwnd;		/* Send congestion window. */
//	u_int32_t	__tcpi_advmss;
//	u_int32_t	__tcpi_reordering;
//
//	u_int32_t	__tcpi_rcv_rtt;
//	u_int32_t	tcpi_rcv_space;		/* Advertised recv window. */
//
//	/* FreeBSD extensions to tcp_info. */
//	u_int32_t	tcpi_snd_wnd;		/* Advertised send window. */
//	u_int32_t	tcpi_snd_bwnd;		/* No longer used. */
//	u_int32_t	tcpi_snd_nxt;		/* Next egress seqno */
//	u_int32_t	tcpi_rcv_nxt;		/* Next ingress seqno */
//	u_int32_t	tcpi_toe_tid;		/* HWTID for TOE endpoints */
//	u_int32_t	tcpi_snd_rexmitpack;	/* Retransmitted packets */
//	u_int32_t	tcpi_rcv_ooopack;	/* Out-of-order packets */
//	u_int32_t	tcpi_snd_zerowin;	/* Zero-sized windows sent */
//
//	/// Padding to grow without breaking ABI.
//    /// Padding.
//	__tcpi_pad: [u32; 26],
//}

/// If this structure is provided when setting the `TCP_FASTOPEN` socket
/// option, and the enable member is non-zero, a subsequent connect will use
/// pre-shared key (PSK) mode using the provided key.
#[repr(C)]
pub struct tcp_fastopen_t {
    pub enable: i32,
    pub psk: [u8; TCP_FASTOPEN_PSK_LEN],
}

pub const TCP_FUNCTION_NAME_LEN_MAX: usize = 32;

#[repr(C)]
pub struct tcp_function_set_t {
    pub function_set_name: [u8; TCP_FUNCTION_NAME_LEN_MAX],
    pub pcbcnt: u32,
}

/// TLS modes for `TCP_TXTLS_MODE`
pub const TCP_TLS_MODE_NONE: i32 = 0;
pub const TCP_TLS_MODE_SW: i32 = 1;
pub const TCP_TLS_MODE_IFNET: i32 = 2;
pub const TCP_TLS_MODE_TOE: i32 = 3;

/// TCP Control message types
pub const TLS_SET_RECORD_TYPE: i32 = 1;
pub const TLS_GET_RECORD: i32 = 2;

/// TCP specific variables of interest for `tp->t_stats` stats(9) accounting.
/// Transmit payload bytes
pub const VOI_TCP_TXPB: i32 = 0;
/// Retransmit payload bytes
pub const VOI_TCP_RETXPB: i32 = 1;
/// Foreign receive window
pub const VOI_TCP_FRWIN: i32 = 2;
/// Local congesiton window
pub const VOI_TCP_LCWIN: i32 = 3;
/// Round trip time
pub const VOI_TCP_RTT: i32 = 4;
/// Congestion signal
pub const VOI_TCP_CSIG: i32 = 5;
/// Goodput
pub const VOI_TCP_GPUT: i32 = 6;
/// Congestion avoidance `LCWIN - FRWIN`
pub const VOI_TCP_CALCFRWINDIFF: i32 = 7;
/// Goodput normalised delta
pub const VOI_TCP_GPUT_ND: i32 = 8;
/// Average `ACKed` bytes per ACK
pub const VOI_TCP_ACKLEN: i32 = 9;

/// remove numa binding
pub const TCP_REUSPORT_LB_NUMA_NODOM: i32 = -2;
/// bind to current domain
pub const TCP_REUSPORT_LB_NUMA_CURDOM: i32 = -1;
