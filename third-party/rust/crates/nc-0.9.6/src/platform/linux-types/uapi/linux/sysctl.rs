// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/sysctl.h`

use crate::size_t;

/// how many path components do we allow in a call to sysctl
/// In other words, what is the largest acceptable value for the nlen member
/// of a struct `sysctl_args_t` to have?
pub const CTL_MAXNAME: i32 = 10;

#[allow(clippy::module_name_repetitions)]
#[repr(C)]
#[derive(Debug, Clone)]
pub struct sysctl_args_t {
    pub name: *mut i32,
    pub nlen: i32,
    pub oldval: usize,
    pub oldlenp: *mut size_t,
    pub newval: usize,
    pub newlen: size_t,
    unused: [usize; 4],
}

/// Top-level names:
///
/// General kernel info and control
pub const CTL_KERN: i32 = 1;
/// VM management
pub const CTL_VM: i32 = 2;
/// Networking
pub const CTL_NET: i32 = 3;
/// removal breaks strace(1) compilation
pub const CTL_PROC: i32 = 4;
/// Filesystems
pub const CTL_FS: i32 = 5;
/// Debugging
pub const CTL_DEBUG: i32 = 6;
/// Devices
pub const CTL_DEV: i32 = 7;
/// Busses
pub const CTL_BUS: i32 = 8;
/// Binary emulation
pub const CTL_ABI: i32 = 9;
/// CPU stuff (speed scaling, etc)
pub const CTL_CPU: i32 = 10;
/// arlan wireless driver
pub const CTL_ARLAN: i32 = 254;
/// s390 debug
pub const CTL_S390DBF: i32 = 5677;
/// sunrpc debug
pub const CTL_SUNRPC: i32 = 7249;
/// frv power management
pub const CTL_PM: i32 = 9899;
/// frv specific sysctls
pub const CTL_FRV: i32 = 9898;

/// `CTL_BUS` names:
pub const CTL_BUS_ISA: i32 = 1;

/// /proc/sys/fs/inotify/
/// max instances per user
pub const INOTIFY_MAX_USER_INSTANCES: i32 = 1;
/// max watches per user
pub const INOTIFY_MAX_USER_WATCHES: i32 = 2;
pub const INOTIFY_MAX_QUEUED_EVENTS: i32 = 3;

/// `CTL_KERN` names:
///
/// string: system version
pub const KERN_OSTYPE: i32 = 1;
/// string: system release
pub const KERN_OSRELEASE: i32 = 2;
/// int: system revision
pub const KERN_OSREV: i32 = 3;
/// string: compile time info
pub const KERN_VERSION: i32 = 4;
/// struct: maximum rights mask
pub const KERN_SECUREMASK: i32 = 5;
/// table: profiling information
pub const KERN_PROF: i32 = 6;
/// string: hostname
pub const KERN_NODENAME: i32 = 7;
/// string: domainname
pub const KERN_DOMAINNAME: i32 = 8;

/// int: panic timeout
pub const KERN_PANIC: i32 = 15;
/// real root device to mount after initrd
pub const KERN_REALROOTDEV: i32 = 16;

/// reboot command on Sparc
pub const KERN_SPARC_REBOOT: i32 = 21;
/// int: allow ctl-alt-del to reboot
pub const KERN_CTLALTDEL: i32 = 22;
/// struct: control printk logging parameters
pub const KERN_PRINTK: i32 = 23;
/// Name translation
pub const KERN_NAMETRANS: i32 = 24;
/// turn htab reclaimation on/off on PPC
pub const KERN_PPC_HTABRECLAIM: i32 = 25;
/// turn idle page zeroing on/off on PPC
pub const KERN_PPC_ZEROPAGED: i32 = 26;
/// use nap mode for power saving
pub const KERN_PPC_POWERSAVE_NAP: i32 = 27;
/// string: modprobe path
pub const KERN_MODPROBE: i32 = 28;
/// int: sg driver reserved buffer size
pub const KERN_SG_BIG_BUFF: i32 = 29;
/// BSD process accounting parameters
pub const KERN_ACCT: i32 = 30;
/// l2cr register on PPC
pub const KERN_PPC_L2CR: i32 = 31;

/// Number of rt sigs queued
pub const KERN_RTSIGNR: i32 = 32;
/// Max queuable
pub const KERN_RTSIGMAX: i32 = 33;

/// long: Maximum shared memory segment
pub const KERN_SHMMAX: i32 = 34;
/// int: Maximum size of a messege
pub const KERN_MSGMAX: i32 = 35;
/// int: Maximum message queue size
pub const KERN_MSGMNB: i32 = 36;
/// int: Maximum system message pool size
pub const KERN_MSGPOOL: i32 = 37;
/// int: Sysreq enable
pub const KERN_SYSRQ: i32 = 38;
/// int: Maximum nr of threads in the system
pub const KERN_MAX_THREADS: i32 = 39;
/// Random driver
pub const KERN_RANDOM: i32 = 40;
/// int: Maximum size of shared memory
pub const KERN_SHMALL: i32 = 41;
/// int: msg queue identifiers
pub const KERN_MSGMNI: i32 = 42;
/// struct: sysv semaphore limits
pub const KERN_SEM: i32 = 43;
/// int: Sparc Stop-A enable
pub const KERN_SPARC_STOP_A: i32 = 44;
/// int: shm array identifiers
pub const KERN_SHMMNI: i32 = 45;
/// int: overflow UID
pub const KERN_OVERFLOWUID: i32 = 46;
/// int: overflow GID
pub const KERN_OVERFLOWGID: i32 = 47;
/// string: path to shm fs
pub const KERN_SHMPATH: i32 = 48;
/// string: path to uevent helper (deprecated)
pub const KERN_HOTPLUG: i32 = 49;
/// int: unimplemented ieee instructions
pub const KERN_IEEE_EMULATION_WARNINGS: i32 = 50;
/// int: dumps of user faults
pub const KERN_S390_USER_DEBUG_LOGGING: i32 = 51;
/// int: use core or core.%pid
pub const KERN_CORE_USES_PID: i32 = 52;
/// int: various kernel tainted flags
pub const KERN_TAINTED: i32 = 53;
/// int: PID of the process to notify on CAD
pub const KERN_CADPID: i32 = 54;
/// int: PID # limit
pub const KERN_PIDMAX: i32 = 55;
/// string: pattern for core-file names
pub const KERN_CORE_PATTERN: i32 = 56;
/// int: whether we will panic on an oops
pub const KERN_PANIC_ON_OOPS: i32 = 57;
/// int: hppa soft-power enable
pub const KERN_HPPA_PWRSW: i32 = 58;
/// int: hppa unaligned-trap enable
pub const KERN_HPPA_UNALIGNED: i32 = 59;
/// int: tune printk ratelimiting
pub const KERN_PRINTK_RATELIMIT: i32 = 60;
/// int: tune printk ratelimiting
pub const KERN_PRINTK_RATELIMIT_BURST: i32 = 61;
/// dir: pty driver
pub const KERN_PTY: i32 = 62;
/// int: `NGROUPS_MAX`
pub const KERN_NGROUPS_MAX: i32 = 63;
/// int: serial console power-off halt
pub const KERN_SPARC_SCONS_PWROFF: i32 = 64;
/// int: hz timer on or off
pub const KERN_HZ_TIMER: i32 = 65;
/// int: unknown nmi panic flag
pub const KERN_UNKNOWN_NMI_PANIC: i32 = 66;
/// int: boot loader type
pub const KERN_BOOTLOADER_TYPE: i32 = 67;
/// int: randomize virtual address space
pub const KERN_RANDOMIZE: i32 = 68;
/// int: behaviour of dumps for setuid core
pub const KERN_SETUID_DUMPABLE: i32 = 69;
/// int: number of spinlock retries
pub const KERN_SPIN_RETRY: i32 = 70;
/// int: flags for setting up video after ACPI sleep
pub const KERN_ACPI_VIDEO_FLAGS: i32 = 71;
/// int: ia64 unaligned userland trap enable
pub const KERN_IA64_UNALIGNED: i32 = 72;
/// int: print compat layer  messages
pub const KERN_COMPAT_LOG: i32 = 73;
/// int: rtmutex's maximum lock depth
pub const KERN_MAX_LOCK_DEPTH: i32 = 74;
/// int: enable/disable nmi watchdog
pub const KERN_NMI_WATCHDOG: i32 = 75;
/// int: whether we will panic on an unrecovered
pub const KERN_PANIC_ON_NMI: i32 = 76;
/// int: call `panic()` in `WARN()` functions
pub const KERN_PANIC_ON_WARN: i32 = 77;
/// ulong: bitmask to print system info on panic
pub const KERN_PANIC_PRINT: i32 = 78;

/// `CTL_VM` names:
///
/// was: struct: Set vm swapping control
pub const VM_UNUSED1: i32 = 1;
/// was; int: Linear or `sqrt()` swapout for hogs
pub const VM_UNUSED2: i32 = 2;
/// was: struct: Set free page thresholds
pub const VM_UNUSED3: i32 = 3;
/// Spare
pub const VM_UNUSED4: i32 = 4;
/// Turn off the virtual memory safety limit
pub const VM_OVERCOMMIT_MEMORY: i32 = 5;
/// was: struct: Set buffer memory thresholds
pub const VM_UNUSED5: i32 = 6;
/// was: struct: Set cache memory thresholds
pub const VM_UNUSED7: i32 = 7;
/// was: struct: Control kswapd behaviour
pub const VM_UNUSED8: i32 = 8;
/// was: struct: Set page table cache parameters
pub const VM_UNUSED9: i32 = 9;
/// int: set number of pages to swap together
pub const VM_PAGE_CLUSTER: i32 = 10;
/// `dirty_background_ratio`
pub const VM_DIRTY_BACKGROUND: i32 = 11;
/// `dirty_ratio`
pub const VM_DIRTY_RATIO: i32 = 12;
/// `dirty_writeback_centisecs`
pub const VM_DIRTY_WB_CS: i32 = 13;
/// `dirty_expire_centisecs`
pub const VM_DIRTY_EXPIRE_CS: i32 = 14;
/// `nr_pdflush_threads`
pub const VM_NR_PDFLUSH_THREADS: i32 = 15;
/// percent of RAM to allow overcommit in
pub const VM_OVERCOMMIT_RATIO: i32 = 16;
/// struct: Control pagebuf parameters
pub const VM_PAGEBUF: i32 = 17;
/// int: Number of available Huge Pages
pub const VM_HUGETLB_PAGES: i32 = 18;
/// Tendency to steal mapped memory
pub const VM_SWAPPINESS: i32 = 19;
/// reservation ratio for lower memory zones
pub const VM_LOWMEM_RESERVE_RATIO: i32 = 20;
/// Minimum free kilobytes to maintain
pub const VM_MIN_FREE_KBYTES: i32 = 21;
/// int: Maximum number of mmaps/address-space
pub const VM_MAX_MAP_COUNT: i32 = 22;
/// vm laptop mode
pub const VM_LAPTOP_MODE: i32 = 23;
/// block dump mode
pub const VM_BLOCK_DUMP: i32 = 24;
/// permitted hugetlb group
pub const VM_HUGETLB_GROUP: i32 = 25;
/// dcache/icache reclaim pressure
pub const VM_VFS_CACHE_PRESSURE: i32 = 26;
/// legacy/compatibility virtual address space layout
pub const VM_LEGACY_VA_LAYOUT: i32 = 27;
/// default time for token time out
pub const VM_SWAP_TOKEN_TIMEOUT: i32 = 28;
/// int: nuke lots of pagecache
pub const VM_DROP_PAGECACHE: i32 = 29;
/// int: fraction of pages in each `percpu_pagelist`
pub const VM_PERCPU_PAGELIST_FRACTION: i32 = 30;
/// reclaim local zone memory before going off node
pub const VM_ZONE_RECLAIM_MODE: i32 = 31;
/// Set min percent of unmapped pages
pub const VM_MIN_UNMAPPED: i32 = 32;
/// panic at out-of-memory
pub const VM_PANIC_ON_OOM: i32 = 33;
/// map VDSO into new processes?
pub const VM_VDSO_ENABLED: i32 = 34;
/// Percent pages ignored by zone reclaim
pub const VM_MIN_SLAB: i32 = 35;

/// `CTL_NET` names:
pub const NET_CORE: i32 = 1;
pub const NET_ETHER: i32 = 2;
pub const NET_802: i32 = 3;
pub const NET_UNIX: i32 = 4;
pub const NET_IPV4: i32 = 5;
pub const NET_IPX: i32 = 6;
pub const NET_ATALK: i32 = 7;
pub const NET_NETROM: i32 = 8;
pub const NET_AX25: i32 = 9;
pub const NET_BRIDGE: i32 = 10;
pub const NET_ROSE: i32 = 11;
pub const NET_IPV6: i32 = 12;
pub const NET_X25: i32 = 13;
pub const NET_TR: i32 = 14;
pub const NET_DECNET: i32 = 15;
pub const NET_ECONET: i32 = 16;
pub const NET_SCTP: i32 = 17;
pub const NET_LLC: i32 = 18;
pub const NET_NETFILTER: i32 = 19;
pub const NET_DCCP: i32 = 20;
pub const NET_IRDA: i32 = 412;

/// `/proc/sys/kernel/random`
pub const RANDOM_POOLSIZE: i32 = 1;
pub const RANDOM_ENTROPY_COUNT: i32 = 2;
pub const RANDOM_READ_THRESH: i32 = 3;
pub const RANDOM_WRITE_THRESH: i32 = 4;
pub const RANDOM_BOOT_ID: i32 = 5;
pub const RANDOM_UUID: i32 = 6;

/// `/proc/sys/kernel/pty`
pub const PTY_MAX: i32 = 1;
pub const PTY_NR: i32 = 2;

/// `/proc/sys/bus/isa`
pub const BUS_ISA_MEM_BASE: i32 = 1;
pub const BUS_ISA_PORT_BASE: i32 = 2;
pub const BUS_ISA_PORT_SHIFT: i32 = 3;

/// `/proc/sys/net/core`
pub const NET_CORE_WMEM_MAX: i32 = 1;
pub const NET_CORE_RMEM_MAX: i32 = 2;
pub const NET_CORE_WMEM_DEFAULT: i32 = 3;
pub const NET_CORE_RMEM_DEFAULT: i32 = 4;
/// was `NET_CORE_DESTROY_DELAY`
pub const NET_CORE_MAX_BACKLOG: i32 = 6;
pub const NET_CORE_FASTROUTE: i32 = 7;
pub const NET_CORE_MSG_COST: i32 = 8;
pub const NET_CORE_MSG_BURST: i32 = 9;
pub const NET_CORE_OPTMEM_MAX: i32 = 10;
pub const NET_CORE_HOT_LIST_LENGTH: i32 = 11;
pub const NET_CORE_DIVERT_VERSION: i32 = 12;
pub const NET_CORE_NO_CONG_THRESH: i32 = 13;
pub const NET_CORE_NO_CONG: i32 = 14;
pub const NET_CORE_LO_CONG: i32 = 15;
pub const NET_CORE_MOD_CONG: i32 = 16;
pub const NET_CORE_DEV_WEIGHT: i32 = 17;
pub const NET_CORE_SOMAXCONN: i32 = 18;
pub const NET_CORE_BUDGET: i32 = 19;
pub const NET_CORE_AEVENT_ETIME: i32 = 20;
pub const NET_CORE_AEVENT_RSEQTH: i32 = 21;
pub const NET_CORE_WARNINGS: i32 = 22;

// /proc/sys/net/ethernet

// /proc/sys/net/802

/// `/proc/sys/net/unix`
pub const NET_UNIX_DESTROY_DELAY: i32 = 1;
pub const NET_UNIX_DELETE_DELAY: i32 = 2;
pub const NET_UNIX_MAX_DGRAM_QLEN: i32 = 3;

/// `/proc/sys/net/netfilter`
pub const NET_NF_CONNTRACK_MAX: i32 = 1;
pub const NET_NF_CONNTRACK_TCP_TIMEOUT_SYN_SENT: i32 = 2;
pub const NET_NF_CONNTRACK_TCP_TIMEOUT_SYN_RECV: i32 = 3;
pub const NET_NF_CONNTRACK_TCP_TIMEOUT_ESTABLISHED: i32 = 4;
pub const NET_NF_CONNTRACK_TCP_TIMEOUT_FIN_WAIT: i32 = 5;
pub const NET_NF_CONNTRACK_TCP_TIMEOUT_CLOSE_WAIT: i32 = 6;
pub const NET_NF_CONNTRACK_TCP_TIMEOUT_LAST_ACK: i32 = 7;
pub const NET_NF_CONNTRACK_TCP_TIMEOUT_TIME_WAIT: i32 = 8;
pub const NET_NF_CONNTRACK_TCP_TIMEOUT_CLOSE: i32 = 9;
pub const NET_NF_CONNTRACK_UDP_TIMEOUT: i32 = 10;
pub const NET_NF_CONNTRACK_UDP_TIMEOUT_STREAM: i32 = 11;
pub const NET_NF_CONNTRACK_ICMP_TIMEOUT: i32 = 12;
pub const NET_NF_CONNTRACK_GENERIC_TIMEOUT: i32 = 13;
pub const NET_NF_CONNTRACK_BUCKETS: i32 = 14;
pub const NET_NF_CONNTRACK_LOG_INVALID: i32 = 15;
pub const NET_NF_CONNTRACK_TCP_TIMEOUT_MAX_RETRANS: i32 = 16;
pub const NET_NF_CONNTRACK_TCP_LOOSE: i32 = 17;
pub const NET_NF_CONNTRACK_TCP_BE_LIBERAL: i32 = 18;
pub const NET_NF_CONNTRACK_TCP_MAX_RETRANS: i32 = 19;
pub const NET_NF_CONNTRACK_SCTP_TIMEOUT_CLOSED: i32 = 20;
pub const NET_NF_CONNTRACK_SCTP_TIMEOUT_COOKIE_WAIT: i32 = 21;
pub const NET_NF_CONNTRACK_SCTP_TIMEOUT_COOKIE_ECHOED: i32 = 22;
pub const NET_NF_CONNTRACK_SCTP_TIMEOUT_ESTABLISHED: i32 = 23;
pub const NET_NF_CONNTRACK_SCTP_TIMEOUT_SHUTDOWN_SENT: i32 = 24;
pub const NET_NF_CONNTRACK_SCTP_TIMEOUT_SHUTDOWN_RECD: i32 = 25;
pub const NET_NF_CONNTRACK_SCTP_TIMEOUT_SHUTDOWN_ACK_SENT: i32 = 26;
pub const NET_NF_CONNTRACK_COUNT: i32 = 27;
pub const NET_NF_CONNTRACK_ICMPV6_TIMEOUT: i32 = 28;
pub const NET_NF_CONNTRACK_FRAG6_TIMEOUT: i32 = 29;
pub const NET_NF_CONNTRACK_FRAG6_LOW_THRESH: i32 = 30;
pub const NET_NF_CONNTRACK_FRAG6_HIGH_THRESH: i32 = 31;
pub const NET_NF_CONNTRACK_CHECKSUM: i32 = 32;

/// `/proc/sys/net/ipv4`
///
/// v2.0 compatibile variables
pub const NET_IPV4_FORWARD: i32 = 8;
pub const NET_IPV4_DYNADDR: i32 = 9;

pub const NET_IPV4_CONF: i32 = 16;
pub const NET_IPV4_NEIGH: i32 = 17;
pub const NET_IPV4_ROUTE: i32 = 18;
pub const NET_IPV4_FIB_HASH: i32 = 19;
pub const NET_IPV4_NETFILTER: i32 = 20;

pub const NET_IPV4_TCP_TIMESTAMPS: i32 = 33;
pub const NET_IPV4_TCP_WINDOW_SCALING: i32 = 34;
pub const NET_IPV4_TCP_SACK: i32 = 35;
pub const NET_IPV4_TCP_RETRANS_COLLAPSE: i32 = 36;
pub const NET_IPV4_DEFAULT_TTL: i32 = 37;
pub const NET_IPV4_AUTOCONFIG: i32 = 38;
pub const NET_IPV4_NO_PMTU_DISC: i32 = 39;
pub const NET_IPV4_TCP_SYN_RETRIES: i32 = 40;
pub const NET_IPV4_IPFRAG_HIGH_THRESH: i32 = 41;
pub const NET_IPV4_IPFRAG_LOW_THRESH: i32 = 42;
pub const NET_IPV4_IPFRAG_TIME: i32 = 43;
pub const NET_IPV4_TCP_MAX_KA_PROBES: i32 = 44;
pub const NET_IPV4_TCP_KEEPALIVE_TIME: i32 = 45;
pub const NET_IPV4_TCP_KEEPALIVE_PROBES: i32 = 46;
pub const NET_IPV4_TCP_RETRIES1: i32 = 47;
pub const NET_IPV4_TCP_RETRIES2: i32 = 48;
pub const NET_IPV4_TCP_FIN_TIMEOUT: i32 = 49;
pub const NET_IPV4_IP_MASQ_DEBUG: i32 = 50;
pub const NET_TCP_SYNCOOKIES: i32 = 51;
pub const NET_TCP_STDURG: i32 = 52;
pub const NET_TCP_RFC1337: i32 = 53;
pub const NET_TCP_SYN_TAILDROP: i32 = 54;
pub const NET_TCP_MAX_SYN_BACKLOG: i32 = 55;
pub const NET_IPV4_LOCAL_PORT_RANGE: i32 = 56;
pub const NET_IPV4_ICMP_ECHO_IGNORE_ALL: i32 = 57;
pub const NET_IPV4_ICMP_ECHO_IGNORE_BROADCASTS: i32 = 58;
pub const NET_IPV4_ICMP_SOURCEQUENCH_RATE: i32 = 59;
pub const NET_IPV4_ICMP_DESTUNREACH_RATE: i32 = 60;
pub const NET_IPV4_ICMP_TIMEEXCEED_RATE: i32 = 61;
pub const NET_IPV4_ICMP_PARAMPROB_RATE: i32 = 62;
pub const NET_IPV4_ICMP_ECHOREPLY_RATE: i32 = 63;
pub const NET_IPV4_ICMP_IGNORE_BOGUS_ERROR_RESPONSES: i32 = 64;
pub const NET_IPV4_IGMP_MAX_MEMBERSHIPS: i32 = 65;
pub const NET_TCP_TW_RECYCLE: i32 = 66;
pub const NET_IPV4_ALWAYS_DEFRAG: i32 = 67;
pub const NET_IPV4_TCP_KEEPALIVE_INTVL: i32 = 68;
pub const NET_IPV4_INET_PEER_THRESHOLD: i32 = 69;
pub const NET_IPV4_INET_PEER_MINTTL: i32 = 70;
pub const NET_IPV4_INET_PEER_MAXTTL: i32 = 71;
pub const NET_IPV4_INET_PEER_GC_MINTIME: i32 = 72;
pub const NET_IPV4_INET_PEER_GC_MAXTIME: i32 = 73;
pub const NET_TCP_ORPHAN_RETRIES: i32 = 74;
pub const NET_TCP_ABORT_ON_OVERFLOW: i32 = 75;
pub const NET_TCP_SYNACK_RETRIES: i32 = 76;
pub const NET_TCP_MAX_ORPHANS: i32 = 77;
pub const NET_TCP_MAX_TW_BUCKETS: i32 = 78;
pub const NET_TCP_FACK: i32 = 79;
pub const NET_TCP_REORDERING: i32 = 80;
pub const NET_TCP_ECN: i32 = 81;
pub const NET_TCP_DSACK: i32 = 82;
pub const NET_TCP_MEM: i32 = 83;
pub const NET_TCP_WMEM: i32 = 84;
pub const NET_TCP_RMEM: i32 = 85;
pub const NET_TCP_APP_WIN: i32 = 86;
pub const NET_TCP_ADV_WIN_SCALE: i32 = 87;
pub const NET_IPV4_NONLOCAL_BIND: i32 = 88;
pub const NET_IPV4_ICMP_RATELIMIT: i32 = 89;
pub const NET_IPV4_ICMP_RATEMASK: i32 = 90;
pub const NET_TCP_TW_REUSE: i32 = 91;
pub const NET_TCP_FRTO: i32 = 92;
pub const NET_TCP_LOW_LATENCY: i32 = 93;
pub const NET_IPV4_IPFRAG_SECRET_INTERVAL: i32 = 94;
pub const NET_IPV4_IGMP_MAX_MSF: i32 = 96;
pub const NET_TCP_NO_METRICS_SAVE: i32 = 97;
pub const NET_TCP_DEFAULT_WIN_SCALE: i32 = 105;
pub const NET_TCP_MODERATE_RCVBUF: i32 = 106;
pub const NET_TCP_TSO_WIN_DIVISOR: i32 = 107;
pub const NET_TCP_BIC_BETA: i32 = 108;
pub const NET_IPV4_ICMP_ERRORS_USE_INBOUND_IFADDR: i32 = 109;
pub const NET_TCP_CONG_CONTROL: i32 = 110;
pub const NET_TCP_ABC: i32 = 111;
pub const NET_IPV4_IPFRAG_MAX_DIST: i32 = 112;
pub const NET_TCP_MTU_PROBING: i32 = 113;
pub const NET_TCP_BASE_MSS: i32 = 114;
pub const NET_IPV4_TCP_WORKAROUND_SIGNED_WINDOWS: i32 = 115;
pub const NET_TCP_DMA_COPYBREAK: i32 = 116;
pub const NET_TCP_SLOW_START_AFTER_IDLE: i32 = 117;
pub const NET_CIPSOV4_CACHE_ENABLE: i32 = 118;
pub const NET_CIPSOV4_CACHE_BUCKET_SIZE: i32 = 119;
pub const NET_CIPSOV4_RBM_OPTFMT: i32 = 120;
pub const NET_CIPSOV4_RBM_STRICTVALID: i32 = 121;
pub const NET_TCP_AVAIL_CONG_CONTROL: i32 = 122;
pub const NET_TCP_ALLOWED_CONG_CONTROL: i32 = 123;
pub const NET_TCP_MAX_SSTHRESH: i32 = 124;
pub const NET_TCP_FRTO_RESPONSE: i32 = 125;

pub const NET_IPV4_ROUTE_FLUSH: i32 = 1;
/// obsolete since 2.6.25
pub const NET_IPV4_ROUTE_MIN_DELAY: i32 = 2;
/// obsolete since 2.6.25
pub const NET_IPV4_ROUTE_MAX_DELAY: i32 = 3;
pub const NET_IPV4_ROUTE_GC_THRESH: i32 = 4;
pub const NET_IPV4_ROUTE_MAX_SIZE: i32 = 5;
pub const NET_IPV4_ROUTE_GC_MIN_INTERVAL: i32 = 6;
pub const NET_IPV4_ROUTE_GC_TIMEOUT: i32 = 7;
/// obsolete since 2.6.38
pub const NET_IPV4_ROUTE_GC_INTERVAL: i32 = 8;
pub const NET_IPV4_ROUTE_REDIRECT_LOAD: i32 = 9;
pub const NET_IPV4_ROUTE_REDIRECT_NUMBER: i32 = 10;
pub const NET_IPV4_ROUTE_REDIRECT_SILENCE: i32 = 11;
pub const NET_IPV4_ROUTE_ERROR_COST: i32 = 12;
pub const NET_IPV4_ROUTE_ERROR_BURST: i32 = 13;
pub const NET_IPV4_ROUTE_GC_ELASTICITY: i32 = 14;
pub const NET_IPV4_ROUTE_MTU_EXPIRES: i32 = 15;
pub const NET_IPV4_ROUTE_MIN_PMTU: i32 = 16;
pub const NET_IPV4_ROUTE_MIN_ADVMSS: i32 = 17;
pub const NET_IPV4_ROUTE_SECRET_INTERVAL: i32 = 18;
pub const NET_IPV4_ROUTE_GC_MIN_INTERVAL_MS: i32 = 19;

pub const NET_PROTO_CONF_ALL: i32 = -2;
pub const NET_PROTO_CONF_DEFAULT: i32 = -3;

pub const NET_IPV4_CONF_FORWARDING: i32 = 1;
pub const NET_IPV4_CONF_MC_FORWARDING: i32 = 2;
pub const NET_IPV4_CONF_PROXY_ARP: i32 = 3;
pub const NET_IPV4_CONF_ACCEPT_REDIRECTS: i32 = 4;
pub const NET_IPV4_CONF_SECURE_REDIRECTS: i32 = 5;
pub const NET_IPV4_CONF_SEND_REDIRECTS: i32 = 6;
pub const NET_IPV4_CONF_SHARED_MEDIA: i32 = 7;
pub const NET_IPV4_CONF_RP_FILTER: i32 = 8;
pub const NET_IPV4_CONF_ACCEPT_SOURCE_ROUTE: i32 = 9;
pub const NET_IPV4_CONF_BOOTP_RELAY: i32 = 10;
pub const NET_IPV4_CONF_LOG_MARTIANS: i32 = 11;
pub const NET_IPV4_CONF_TAG: i32 = 12;
pub const NET_IPV4_CONF_ARPFILTER: i32 = 13;
pub const NET_IPV4_CONF_MEDIUM_ID: i32 = 14;
pub const NET_IPV4_CONF_NOXFRM: i32 = 15;
pub const NET_IPV4_CONF_NOPOLICY: i32 = 16;
pub const NET_IPV4_CONF_FORCE_IGMP_VERSION: i32 = 17;
pub const NET_IPV4_CONF_ARP_ANNOUNCE: i32 = 18;
pub const NET_IPV4_CONF_ARP_IGNORE: i32 = 19;
pub const NET_IPV4_CONF_PROMOTE_SECONDARIES: i32 = 20;
pub const NET_IPV4_CONF_ARP_ACCEPT: i32 = 21;
pub const NET_IPV4_CONF_ARP_NOTIFY: i32 = 22;

/// `/proc/sys/net/ipv4/netfilter`
pub const NET_IPV4_NF_CONNTRACK_MAX: i32 = 1;
pub const NET_IPV4_NF_CONNTRACK_TCP_TIMEOUT_SYN_SENT: i32 = 2;
pub const NET_IPV4_NF_CONNTRACK_TCP_TIMEOUT_SYN_RECV: i32 = 3;
pub const NET_IPV4_NF_CONNTRACK_TCP_TIMEOUT_ESTABLISHED: i32 = 4;
pub const NET_IPV4_NF_CONNTRACK_TCP_TIMEOUT_FIN_WAIT: i32 = 5;
pub const NET_IPV4_NF_CONNTRACK_TCP_TIMEOUT_CLOSE_WAIT: i32 = 6;
pub const NET_IPV4_NF_CONNTRACK_TCP_TIMEOUT_LAST_ACK: i32 = 7;
pub const NET_IPV4_NF_CONNTRACK_TCP_TIMEOUT_TIME_WAIT: i32 = 8;
pub const NET_IPV4_NF_CONNTRACK_TCP_TIMEOUT_CLOSE: i32 = 9;
pub const NET_IPV4_NF_CONNTRACK_UDP_TIMEOUT: i32 = 10;
pub const NET_IPV4_NF_CONNTRACK_UDP_TIMEOUT_STREAM: i32 = 11;
pub const NET_IPV4_NF_CONNTRACK_ICMP_TIMEOUT: i32 = 12;
pub const NET_IPV4_NF_CONNTRACK_GENERIC_TIMEOUT: i32 = 13;
pub const NET_IPV4_NF_CONNTRACK_BUCKETS: i32 = 14;
pub const NET_IPV4_NF_CONNTRACK_LOG_INVALID: i32 = 15;
pub const NET_IPV4_NF_CONNTRACK_TCP_TIMEOUT_MAX_RETRANS: i32 = 16;
pub const NET_IPV4_NF_CONNTRACK_TCP_LOOSE: i32 = 17;
pub const NET_IPV4_NF_CONNTRACK_TCP_BE_LIBERAL: i32 = 18;
pub const NET_IPV4_NF_CONNTRACK_TCP_MAX_RETRANS: i32 = 19;
pub const NET_IPV4_NF_CONNTRACK_SCTP_TIMEOUT_CLOSED: i32 = 20;
pub const NET_IPV4_NF_CONNTRACK_SCTP_TIMEOUT_COOKIE_WAIT: i32 = 21;
pub const NET_IPV4_NF_CONNTRACK_SCTP_TIMEOUT_COOKIE_ECHOED: i32 = 22;
pub const NET_IPV4_NF_CONNTRACK_SCTP_TIMEOUT_ESTABLISHED: i32 = 23;
pub const NET_IPV4_NF_CONNTRACK_SCTP_TIMEOUT_SHUTDOWN_SENT: i32 = 24;
pub const NET_IPV4_NF_CONNTRACK_SCTP_TIMEOUT_SHUTDOWN_RECD: i32 = 25;
pub const NET_IPV4_NF_CONNTRACK_SCTP_TIMEOUT_SHUTDOWN_ACK_SENT: i32 = 26;
pub const NET_IPV4_NF_CONNTRACK_COUNT: i32 = 27;
pub const NET_IPV4_NF_CONNTRACK_CHECKSUM: i32 = 28;

/// `/proc/sys/net/ipv6`
pub const NET_IPV6_CONF: i32 = 16;
pub const NET_IPV6_NEIGH: i32 = 17;
pub const NET_IPV6_ROUTE: i32 = 18;
pub const NET_IPV6_ICMP: i32 = 19;
pub const NET_IPV6_BINDV6ONLY: i32 = 20;
pub const NET_IPV6_IP6FRAG_HIGH_THRESH: i32 = 21;
pub const NET_IPV6_IP6FRAG_LOW_THRESH: i32 = 22;
pub const NET_IPV6_IP6FRAG_TIME: i32 = 23;
pub const NET_IPV6_IP6FRAG_SECRET_INTERVAL: i32 = 24;
pub const NET_IPV6_MLD_MAX_MSF: i32 = 25;

pub const NET_IPV6_ROUTE_FLUSH: i32 = 1;
pub const NET_IPV6_ROUTE_GC_THRESH: i32 = 2;
pub const NET_IPV6_ROUTE_MAX_SIZE: i32 = 3;
pub const NET_IPV6_ROUTE_GC_MIN_INTERVAL: i32 = 4;
pub const NET_IPV6_ROUTE_GC_TIMEOUT: i32 = 5;
pub const NET_IPV6_ROUTE_GC_INTERVAL: i32 = 6;
pub const NET_IPV6_ROUTE_GC_ELASTICITY: i32 = 7;
pub const NET_IPV6_ROUTE_MTU_EXPIRES: i32 = 8;
pub const NET_IPV6_ROUTE_MIN_ADVMSS: i32 = 9;
pub const NET_IPV6_ROUTE_GC_MIN_INTERVAL_MS: i32 = 10;

pub const NET_IPV6_FORWARDING: i32 = 1;
pub const NET_IPV6_HOP_LIMIT: i32 = 2;
pub const NET_IPV6_MTU: i32 = 3;
pub const NET_IPV6_ACCEPT_RA: i32 = 4;
pub const NET_IPV6_ACCEPT_REDIRECTS: i32 = 5;
pub const NET_IPV6_AUTOCONF: i32 = 6;
pub const NET_IPV6_DAD_TRANSMITS: i32 = 7;
pub const NET_IPV6_RTR_SOLICITS: i32 = 8;
pub const NET_IPV6_RTR_SOLICIT_INTERVAL: i32 = 9;
pub const NET_IPV6_RTR_SOLICIT_DELAY: i32 = 10;
pub const NET_IPV6_USE_TEMPADDR: i32 = 11;
pub const NET_IPV6_TEMP_VALID_LFT: i32 = 12;
pub const NET_IPV6_TEMP_PREFERED_LFT: i32 = 13;
pub const NET_IPV6_REGEN_MAX_RETRY: i32 = 14;
pub const NET_IPV6_MAX_DESYNC_FACTOR: i32 = 15;
pub const NET_IPV6_MAX_ADDRESSES: i32 = 16;
pub const NET_IPV6_FORCE_MLD_VERSION: i32 = 17;
pub const NET_IPV6_ACCEPT_RA_DEFRTR: i32 = 18;
pub const NET_IPV6_ACCEPT_RA_PINFO: i32 = 19;
pub const NET_IPV6_ACCEPT_RA_RTR_PREF: i32 = 20;
pub const NET_IPV6_RTR_PROBE_INTERVAL: i32 = 21;
pub const NET_IPV6_ACCEPT_RA_RT_INFO_MAX_PLEN: i32 = 22;
pub const NET_IPV6_PROXY_NDP: i32 = 23;
pub const NET_IPV6_ACCEPT_SOURCE_ROUTE: i32 = 25;
pub const NET_IPV6_ACCEPT_RA_FROM_LOCAL: i32 = 26;
pub const NET_IPV6_ACCEPT_RA_RT_INFO_MIN_PLEN: i32 = 27;
pub const NET_IPV6_MAX: i32 = 28;

/// `/proc/sys/net/ipv6/icmp`
pub const NET_IPV6_ICMP_RATELIMIT: i32 = 1;
pub const NET_IPV6_ICMP_ECHO_IGNORE_ALL: i32 = 2;

/// `/proc/sys/net/<protocol>/neigh/<dev>`
pub const NET_NEIGH_MCAST_SOLICIT: i32 = 1;
pub const NET_NEIGH_UCAST_SOLICIT: i32 = 2;
pub const NET_NEIGH_APP_SOLICIT: i32 = 3;
pub const NET_NEIGH_RETRANS_TIME: i32 = 4;
pub const NET_NEIGH_REACHABLE_TIME: i32 = 5;
pub const NET_NEIGH_DELAY_PROBE_TIME: i32 = 6;
pub const NET_NEIGH_GC_STALE_TIME: i32 = 7;
pub const NET_NEIGH_UNRES_QLEN: i32 = 8;
pub const NET_NEIGH_PROXY_QLEN: i32 = 9;
pub const NET_NEIGH_ANYCAST_DELAY: i32 = 10;
pub const NET_NEIGH_PROXY_DELAY: i32 = 11;
pub const NET_NEIGH_LOCKTIME: i32 = 12;
pub const NET_NEIGH_GC_INTERVAL: i32 = 13;
pub const NET_NEIGH_GC_THRESH1: i32 = 14;
pub const NET_NEIGH_GC_THRESH2: i32 = 15;
pub const NET_NEIGH_GC_THRESH3: i32 = 16;
pub const NET_NEIGH_RETRANS_TIME_MS: i32 = 17;
pub const NET_NEIGH_REACHABLE_TIME_MS: i32 = 18;

/// `/proc/sys/net/dccp`
pub const NET_DCCP_DEFAULT: i32 = 1;

/// `/proc/sys/net/ipx`
pub const NET_IPX_PPROP_BROADCASTING: i32 = 1;
pub const NET_IPX_FORWARDING: i32 = 2;

/// `/proc/sys/net/llc`
pub const NET_LLC2: i32 = 1;
pub const NET_LLC_STATION: i32 = 2;

/// `/proc/sys/net/llc/llc2`
pub const NET_LLC2_TIMEOUT: i32 = 1;

/// `/proc/sys/net/llc/station`
pub const NET_LLC_STATION_ACK_TIMEOUT: i32 = 1;

/// `/proc/sys/net/llc/llc2/timeout`
pub const NET_LLC2_ACK_TIMEOUT: i32 = 1;
pub const NET_LLC2_P_TIMEOUT: i32 = 2;
pub const NET_LLC2_REJ_TIMEOUT: i32 = 3;
pub const NET_LLC2_BUSY_TIMEOUT: i32 = 4;

/// `/proc/sys/net/appletalk`
pub const NET_ATALK_AARP_EXPIRY_TIME: i32 = 1;
pub const NET_ATALK_AARP_TICK_TIME: i32 = 2;
pub const NET_ATALK_AARP_RETRANSMIT_LIMIT: i32 = 3;
pub const NET_ATALK_AARP_RESOLVE_TIME: i32 = 4;

/// `/proc/sys/net/netrom`
pub const NET_NETROM_DEFAULT_PATH_QUALITY: i32 = 1;
pub const NET_NETROM_OBSOLESCENCE_COUNT_INITIALISER: i32 = 2;
pub const NET_NETROM_NETWORK_TTL_INITIALISER: i32 = 3;
pub const NET_NETROM_TRANSPORT_TIMEOUT: i32 = 4;
pub const NET_NETROM_TRANSPORT_MAXIMUM_TRIES: i32 = 5;
pub const NET_NETROM_TRANSPORT_ACKNOWLEDGE_DELAY: i32 = 6;
pub const NET_NETROM_TRANSPORT_BUSY_DELAY: i32 = 7;
pub const NET_NETROM_TRANSPORT_REQUESTED_WINDOW_SIZE: i32 = 8;
pub const NET_NETROM_TRANSPORT_NO_ACTIVITY_TIMEOUT: i32 = 9;
pub const NET_NETROM_ROUTING_CONTROL: i32 = 10;
pub const NET_NETROM_LINK_FAILS_COUNT: i32 = 11;
pub const NET_NETROM_RESET: i32 = 12;

/// `/proc/sys/net/ax25`
pub const NET_AX25_IP_DEFAULT_MODE: i32 = 1;
pub const NET_AX25_DEFAULT_MODE: i32 = 2;
pub const NET_AX25_BACKOFF_TYPE: i32 = 3;
pub const NET_AX25_CONNECT_MODE: i32 = 4;
pub const NET_AX25_STANDARD_WINDOW: i32 = 5;
pub const NET_AX25_EXTENDED_WINDOW: i32 = 6;
pub const NET_AX25_T1_TIMEOUT: i32 = 7;
pub const NET_AX25_T2_TIMEOUT: i32 = 8;
pub const NET_AX25_T3_TIMEOUT: i32 = 9;
pub const NET_AX25_IDLE_TIMEOUT: i32 = 10;
pub const NET_AX25_N2: i32 = 11;
pub const NET_AX25_PACLEN: i32 = 12;
pub const NET_AX25_PROTOCOL: i32 = 13;
pub const NET_AX25_DAMA_SLAVE_TIMEOUT: i32 = 14;

/// `/proc/sys/net/rose`
pub const NET_ROSE_RESTART_REQUEST_TIMEOUT: i32 = 1;
pub const NET_ROSE_CALL_REQUEST_TIMEOUT: i32 = 2;
pub const NET_ROSE_RESET_REQUEST_TIMEOUT: i32 = 3;
pub const NET_ROSE_CLEAR_REQUEST_TIMEOUT: i32 = 4;
pub const NET_ROSE_ACK_HOLD_BACK_TIMEOUT: i32 = 5;
pub const NET_ROSE_ROUTING_CONTROL: i32 = 6;
pub const NET_ROSE_LINK_FAIL_TIMEOUT: i32 = 7;
pub const NET_ROSE_MAX_VCS: i32 = 8;
pub const NET_ROSE_WINDOW_SIZE: i32 = 9;
pub const NET_ROSE_NO_ACTIVITY_TIMEOUT: i32 = 10;

/// `/proc/sys/net/x25`
pub const NET_X25_RESTART_REQUEST_TIMEOUT: i32 = 1;
pub const NET_X25_CALL_REQUEST_TIMEOUT: i32 = 2;
pub const NET_X25_RESET_REQUEST_TIMEOUT: i32 = 3;
pub const NET_X25_CLEAR_REQUEST_TIMEOUT: i32 = 4;
pub const NET_X25_ACK_HOLD_BACK_TIMEOUT: i32 = 5;
pub const NET_X25_FORWARD: i32 = 6;

/// `/proc/sys/net/token-ring`
pub const NET_TR_RIF_TIMEOUT: i32 = 1;

/// `/proc/sys/net/decnet/`
pub const NET_DECNET_NODE_TYPE: i32 = 1;
pub const NET_DECNET_NODE_ADDRESS: i32 = 2;
pub const NET_DECNET_NODE_NAME: i32 = 3;
pub const NET_DECNET_DEFAULT_DEVICE: i32 = 4;
pub const NET_DECNET_TIME_WAIT: i32 = 5;
pub const NET_DECNET_DN_COUNT: i32 = 6;
pub const NET_DECNET_DI_COUNT: i32 = 7;
pub const NET_DECNET_DR_COUNT: i32 = 8;
pub const NET_DECNET_DST_GC_INTERVAL: i32 = 9;
pub const NET_DECNET_CONF: i32 = 10;
pub const NET_DECNET_NO_FC_MAX_CWND: i32 = 11;
pub const NET_DECNET_MEM: i32 = 12;
pub const NET_DECNET_RMEM: i32 = 13;
pub const NET_DECNET_WMEM: i32 = 14;
pub const NET_DECNET_DEBUG_LEVEL: i32 = 255;

/// `/proc/sys/net/decnet/conf/<dev>`
pub const NET_DECNET_CONF_LOOPBACK: i32 = -2;
pub const NET_DECNET_CONF_DDCMP: i32 = -3;
pub const NET_DECNET_CONF_PPP: i32 = -4;
pub const NET_DECNET_CONF_X25: i32 = -5;
pub const NET_DECNET_CONF_GRE: i32 = -6;
pub const NET_DECNET_CONF_ETHER: i32 = -7;

/// `/proc/sys/net/decnet/conf/<dev>/`
pub const NET_DECNET_CONF_DEV_PRIORITY: i32 = 1;
pub const NET_DECNET_CONF_DEV_T1: i32 = 2;
pub const NET_DECNET_CONF_DEV_T2: i32 = 3;
pub const NET_DECNET_CONF_DEV_T3: i32 = 4;
pub const NET_DECNET_CONF_DEV_FORWARDING: i32 = 5;
pub const NET_DECNET_CONF_DEV_BLKSIZE: i32 = 6;
pub const NET_DECNET_CONF_DEV_STATE: i32 = 7;

/// `/proc/sys/net/sctp`
pub const NET_SCTP_RTO_INITIAL: i32 = 1;
pub const NET_SCTP_RTO_MIN: i32 = 2;
pub const NET_SCTP_RTO_MAX: i32 = 3;
pub const NET_SCTP_RTO_ALPHA: i32 = 4;
pub const NET_SCTP_RTO_BETA: i32 = 5;
pub const NET_SCTP_VALID_COOKIE_LIFE: i32 = 6;
pub const NET_SCTP_ASSOCIATION_MAX_RETRANS: i32 = 7;
pub const NET_SCTP_PATH_MAX_RETRANS: i32 = 8;
pub const NET_SCTP_MAX_INIT_RETRANSMITS: i32 = 9;
pub const NET_SCTP_HB_INTERVAL: i32 = 10;
pub const NET_SCTP_PRESERVE_ENABLE: i32 = 11;
pub const NET_SCTP_MAX_BURST: i32 = 12;
pub const NET_SCTP_ADDIP_ENABLE: i32 = 13;
pub const NET_SCTP_PRSCTP_ENABLE: i32 = 14;
pub const NET_SCTP_SNDBUF_POLICY: i32 = 15;
pub const NET_SCTP_SACK_TIMEOUT: i32 = 16;
pub const NET_SCTP_RCVBUF_POLICY: i32 = 17;

/// `/proc/sys/net/bridge`
pub const NET_BRIDGE_NF_CALL_ARPTABLES: i32 = 1;
pub const NET_BRIDGE_NF_CALL_IPTABLES: i32 = 2;
pub const NET_BRIDGE_NF_CALL_IP6TABLES: i32 = 3;
pub const NET_BRIDGE_NF_FILTER_VLAN_TAGGED: i32 = 4;
pub const NET_BRIDGE_NF_FILTER_PPPOE_TAGGED: i32 = 5;

/// `CTL_FS` names:
///
/// int:current number of allocated inodes
pub const FS_NRINODE: i32 = 1;
pub const FS_STATINODE: i32 = 2;
/// int:maximum number of inodes that can be allocated
pub const FS_MAXINODE: i32 = 3;
/// int:current number of allocated dquots
pub const FS_NRDQUOT: i32 = 4;
/// int:maximum number of dquots that can be allocated
pub const FS_MAXDQUOT: i32 = 5;
/// int:current number of allocated filedescriptors
pub const FS_NRFILE: i32 = 6;
/// int:maximum number of filedescriptors that can be allocated
pub const FS_MAXFILE: i32 = 7;
pub const FS_DENTRY: i32 = 8;
/// int:current number of allocated `super_blocks`
pub const FS_NRSUPER: i32 = 9;
/// int:maximum number of `super_blocks` that can be allocated
pub const FS_MAXSUPER: i32 = 10;
/// int: overflow UID
pub const FS_OVERFLOWUID: i32 = 11;
/// int: overflow GID
pub const FS_OVERFLOWGID: i32 = 12;
/// int: leases enabled
pub const FS_LEASES: i32 = 13;
/// int: directory notification enabled
pub const FS_DIR_NOTIFY: i32 = 14;
/// int: maximum time to wait for a lease break
pub const FS_LEASE_TIME: i32 = 15;
/// disc quota usage statistics and control
pub const FS_DQSTATS: i32 = 16;
/// struct: control xfs parameters
pub const FS_XFS: i32 = 17;
/// current system-wide number of aio requests
pub const FS_AIO_NR: i32 = 18;
/// system-wide maximum number of aio requests
pub const FS_AIO_MAX_NR: i32 = 19;
/// inotify submenu
pub const FS_INOTIFY: i32 = 20;
/// ocfs2
pub const FS_OCFS2: i32 = 988;

/// `/proc/sys/fs/quota/`
pub const FS_DQ_LOOKUPS: i32 = 1;
pub const FS_DQ_DROPS: i32 = 2;
pub const FS_DQ_READS: i32 = 3;
pub const FS_DQ_WRITES: i32 = 4;
pub const FS_DQ_CACHE_HITS: i32 = 5;
pub const FS_DQ_ALLOCATED: i32 = 6;
pub const FS_DQ_FREE: i32 = 7;
pub const FS_DQ_SYNCS: i32 = 8;
pub const FS_DQ_WARNINGS: i32 = 9;

/// `CTL_DEV` names:
pub const DEV_CDROM: i32 = 1;
pub const DEV_HWMON: i32 = 2;
pub const DEV_PARPORT: i32 = 3;
pub const DEV_RAID: i32 = 4;
pub const DEV_MAC_HID: i32 = 5;
pub const DEV_SCSI: i32 = 6;
pub const DEV_IPMI: i32 = 7;

/// `/proc/sys/dev/cdrom`
pub const DEV_CDROM_INFO: i32 = 1;
pub const DEV_CDROM_AUTOCLOSE: i32 = 2;
pub const DEV_CDROM_AUTOEJECT: i32 = 3;
pub const DEV_CDROM_DEBUG: i32 = 4;
pub const DEV_CDROM_LOCK: i32 = 5;
pub const DEV_CDROM_CHECK_MEDIA: i32 = 6;

/// `/proc/sys/dev/parport`
pub const DEV_PARPORT_DEFAULT: i32 = -3;

/// `/proc/sys/dev/raid`
pub const DEV_RAID_SPEED_LIMIT_MIN: i32 = 1;
pub const DEV_RAID_SPEED_LIMIT_MAX: i32 = 2;

/// `/proc/sys/dev/parport/default`
pub const DEV_PARPORT_DEFAULT_TIMESLICE: i32 = 1;
pub const DEV_PARPORT_DEFAULT_SPINTIME: i32 = 2;

/// `/proc/sys/dev/parport/parport n`
pub const DEV_PARPORT_SPINTIME: i32 = 1;
pub const DEV_PARPORT_BASE_ADDR: i32 = 2;
pub const DEV_PARPORT_IRQ: i32 = 3;
pub const DEV_PARPORT_DMA: i32 = 4;
pub const DEV_PARPORT_MODES: i32 = 5;
pub const DEV_PARPORT_DEVICES: i32 = 6;
pub const DEV_PARPORT_AUTOPROBE: i32 = 16;

/// `/proc/sys/dev/parport/parport n/devices/`
pub const DEV_PARPORT_DEVICES_ACTIVE: i32 = -3;

/// `/proc/sys/dev/parport/parport n /devices/device n`
pub const DEV_PARPORT_DEVICE_TIMESLICE: i32 = 1;

/// `/proc/sys/dev/mac_hid`
pub const DEV_MAC_HID_KEYBOARD_SENDS_LINUX_KEYCODES: i32 = 1;
pub const DEV_MAC_HID_KEYBOARD_LOCK_KEYCODES: i32 = 2;
pub const DEV_MAC_HID_MOUSE_BUTTON_EMULATION: i32 = 3;
pub const DEV_MAC_HID_MOUSE_BUTTON2_KEYCODE: i32 = 4;
pub const DEV_MAC_HID_MOUSE_BUTTON3_KEYCODE: i32 = 5;
pub const DEV_MAC_HID_ADB_MOUSE_SENDS_KEYCODES: i32 = 6;

/// `/proc/sys/dev/scsi`
pub const DEV_SCSI_LOGGING_LEVEL: i32 = 1;

/// `/proc/sys/dev/ipmi`
pub const DEV_IPMI_POWEROFF_POWERCYCLE: i32 = 1;

/// `/proc/sys/abi`
///
/// default handler for coff binaries
pub const ABI_DEFHANDLER_COFF: i32 = 1;
/// default handler for ELF binaries
pub const ABI_DEFHANDLER_ELF: i32 = 2;
/// default handler for procs using lcall7
pub const ABI_DEFHANDLER_LCALL7: i32 = 3;
/// default handler for an libc.so ELF interp
pub const ABI_DEFHANDLER_LIBCSO: i32 = 4;
/// tracing flags
pub const ABI_TRACE: i32 = 5;
/// fake target utsname information
pub const ABI_FAKE_UTSNAME: i32 = 6;
