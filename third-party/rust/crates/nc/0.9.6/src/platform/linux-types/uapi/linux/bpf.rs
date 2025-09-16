// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/bpf.h`

#![allow(clippy::module_name_repetitions)]

use crate::{be16_t, be32_t};

/// Extended instruction set based on top of classic BPF
///
/// instruction classes
/// jmp mode in word width
pub const BPF_JMP32: i32 = 0x06;
/// alu mode in double word width
pub const BPF_ALU64: i32 = 0x07;

/// ld/ldx fields
/// double word (64-bit)
pub const BPF_DW: i32 = 0x18;
/// exclusive add
pub const BPF_XADD: i32 = 0xc0;

/// alu/jmp fields
/// mov reg to reg
pub const BPF_MOV: i32 = 0xb0;
/// sign extending arithmetic shift right
pub const BPF_ARSH: i32 = 0xc0;

/// change endianness of a register
/// flags for endianness conversion:
pub const BPF_END: i32 = 0xd0;
/// convert to little-endian
pub const BPF_TO_LE: i32 = 0x00;
/// convert to big-endian
pub const BPF_TO_BE: i32 = 0x08;
pub const BPF_FROM_LE: i32 = BPF_TO_LE;
pub const BPF_FROM_BE: i32 = BPF_TO_BE;

/// jmp encodings
/// jump !=
pub const BPF_JNE: i32 = 0x50;
/// LT is unsigned, '<'
pub const BPF_JLT: i32 = 0xa0;
/// LE is unsigned, '<='
pub const BPF_JLE: i32 = 0xb0;
/// SGT is signed '>', GT in x86
pub const BPF_JSGT: i32 = 0x60;
/// SGE is signed '>=', GE in x86
pub const BPF_JSGE: i32 = 0x70;
/// SLT is signed, '<'
pub const BPF_JSLT: i32 = 0xc0;
/// SLE is signed, '<='
pub const BPF_JSLE: i32 = 0xd0;
/// function call
pub const BPF_CALL: i32 = 0x80;
/// function return
pub const BPF_EXIT: i32 = 0x90;

/// Register numbers
pub const BPF_REG_0: i32 = 0;
pub const BPF_REG_1: i32 = 1;
pub const BPF_REG_2: i32 = 2;
pub const BPF_REG_3: i32 = 3;
pub const BPF_REG_4: i32 = 4;
pub const BPF_REG_5: i32 = 5;
pub const BPF_REG_6: i32 = 6;
pub const BPF_REG_7: i32 = 7;
pub const BPF_REG_8: i32 = 8;
pub const BPF_REG_9: i32 = 9;
pub const BPF_REG_10: i32 = 10;
/// BPF has 10 general purpose 64-bit registers and stack frame.
pub const MAX_BPF_REG: i32 = BPF_REG_10 + 1;

#[repr(C)]
#[derive(Debug, Default)]
pub struct bpf_insn_t {
    /// opcode
    pub code: u8,

    //pub dst_reg: u8,
    //pub src_reg: u8,
    /// dest register
    pub dst_reg: u32,

    /// source register
    pub src_reg: u32,

    /// signed offset
    pub off: i16,

    /// signed immediate constant
    pub imm: i32,
}

/// Key of a `BPF_MAP_TYPE_LPM_TRIE` entry
#[repr(C)]
#[derive(Debug, Default)]
pub struct bpf_lpm_trie_key_t {
    /// up to 32 for `AF_INET`, 128 for `AF_INET6`
    pub prefixlen: u32,

    /// Arbitrary size
    pub data: [u8; 0],
}

#[repr(C)]
#[derive(Debug, Default)]
pub struct bpf_cgroup_storage_key_t {
    /// cgroup inode id
    pub cgroup_inode_id: u64,

    /// program attach type
    pub attach_type: u32,
}

/// BPF syscall commands, see `bpf(2)` man-page for details.
pub const BPF_MAP_CREATE: i32 = 0;
pub const BPF_MAP_LOOKUP_ELEM: i32 = 1;
pub const BPF_MAP_UPDATE_ELEM: i32 = 2;
pub const BPF_MAP_DELETE_ELEM: i32 = 3;
pub const BPF_MAP_GET_NEXT_KEY: i32 = 4;
pub const BPF_PROG_LOAD: i32 = 5;
pub const BPF_OBJ_PIN: i32 = 6;
pub const BPF_OBJ_GET: i32 = 7;
pub const BPF_PROG_ATTACH: i32 = 8;
pub const BPF_PROG_DETACH: i32 = 9;
pub const BPF_PROG_TEST_RUN: i32 = 10;
pub const BPF_PROG_GET_NEXT_ID: i32 = 11;
pub const BPF_MAP_GET_NEXT_ID: i32 = 12;
pub const BPF_PROG_GET_FD_BY_ID: i32 = 13;
pub const BPF_MAP_GET_FD_BY_ID: i32 = 14;
pub const BPF_OBJ_GET_INFO_BY_FD: i32 = 15;
pub const BPF_PROG_QUERY: i32 = 16;
pub const BPF_RAW_TRACEPOINT_OPEN: i32 = 17;
pub const BPF_BTF_LOAD: i32 = 18;
pub const BPF_BTF_GET_FD_BY_ID: i32 = 19;
pub const BPF_TASK_FD_QUERY: i32 = 20;
pub const BPF_MAP_LOOKUP_AND_DELETE_ELEM: i32 = 21;

pub const BPF_MAP_TYPE_UNSPEC: i32 = 0;
pub const BPF_MAP_TYPE_HASH: i32 = 1;
pub const BPF_MAP_TYPE_ARRAY: i32 = 2;
pub const BPF_MAP_TYPE_PROG_ARRAY: i32 = 3;
pub const BPF_MAP_TYPE_PERF_EVENT_ARRAY: i32 = 4;
pub const BPF_MAP_TYPE_PERCPU_HASH: i32 = 5;
pub const BPF_MAP_TYPE_PERCPU_ARRAY: i32 = 6;
pub const BPF_MAP_TYPE_STACK_TRACE: i32 = 7;
pub const BPF_MAP_TYPE_CGROUP_ARRAY: i32 = 8;
pub const BPF_MAP_TYPE_LRU_HASH: i32 = 9;
pub const BPF_MAP_TYPE_LRU_PERCPU_HASH: i32 = 10;
pub const BPF_MAP_TYPE_LPM_TRIE: i32 = 11;
pub const BPF_MAP_TYPE_ARRAY_OF_MAPS: i32 = 12;
pub const BPF_MAP_TYPE_HASH_OF_MAPS: i32 = 13;
pub const BPF_MAP_TYPE_DEVMAP: i32 = 14;
pub const BPF_MAP_TYPE_SOCKMAP: i32 = 15;
pub const BPF_MAP_TYPE_CPUMAP: i32 = 16;
pub const BPF_MAP_TYPE_XSKMAP: i32 = 17;
pub const BPF_MAP_TYPE_SOCKHASH: i32 = 18;
pub const BPF_MAP_TYPE_CGROUP_STORAGE: i32 = 19;
pub const BPF_MAP_TYPE_REUSEPORT_SOCKARRAY: i32 = 20;
pub const BPF_MAP_TYPE_PERCPU_CGROUP_STORAGE: i32 = 21;
pub const BPF_MAP_TYPE_QUEUE: i32 = 22;
pub const BPF_MAP_TYPE_STACK: i32 = 23;

/// Program types of bpf.
///
/// Note that tracing related programs such as
/// `BPF_PROG_TYPE_{KPROBE,TRACEPOINT,PERF_EVENT,RAW_TRACEPOINT}`
/// are not subject to a stable API since kernel internal data
/// structures can change from release to release and may
/// therefore break existing tracing BPF programs. Tracing BPF
/// programs correspond to a specific kernel which is to be
/// analyzed, and not a specific kernel and all future ones.
pub const BPF_PROG_TYPE_UNSPEC: i32 = 0;
pub const BPF_PROG_TYPE_SOCKET_FILTER: i32 = 1;
pub const BPF_PROG_TYPE_KPROBE: i32 = 2;
pub const BPF_PROG_TYPE_SCHED_CLS: i32 = 3;
pub const BPF_PROG_TYPE_SCHED_ACT: i32 = 4;
pub const BPF_PROG_TYPE_TRACEPOINT: i32 = 5;
pub const BPF_PROG_TYPE_XDP: i32 = 6;
pub const BPF_PROG_TYPE_PERF_EVENT: i32 = 7;
pub const BPF_PROG_TYPE_CGROUP_SKB: i32 = 8;
pub const BPF_PROG_TYPE_CGROUP_SOCK: i32 = 9;
pub const BPF_PROG_TYPE_LWT_IN: i32 = 10;
pub const BPF_PROG_TYPE_LWT_OUT: i32 = 11;
pub const BPF_PROG_TYPE_LWT_XMIT: i32 = 12;
pub const BPF_PROG_TYPE_SOCK_OPS: i32 = 13;
pub const BPF_PROG_TYPE_SK_SKB: i32 = 14;
pub const BPF_PROG_TYPE_CGROUP_DEVICE: i32 = 15;
pub const BPF_PROG_TYPE_SK_MSG: i32 = 16;
pub const BPF_PROG_TYPE_RAW_TRACEPOINT: i32 = 17;
pub const BPF_PROG_TYPE_CGROUP_SOCK_ADDR: i32 = 18;
pub const BPF_PROG_TYPE_LWT_SEG6LOCAL: i32 = 19;
pub const BPF_PROG_TYPE_LIRC_MODE2: i32 = 20;
pub const BPF_PROG_TYPE_SK_REUSEPORT: i32 = 21;
pub const BPF_PROG_TYPE_FLOW_DISSECTOR: i32 = 22;

pub const BPF_CGROUP_INET_INGRESS: i32 = 0;
pub const BPF_CGROUP_INET_EGRESS: i32 = 1;
pub const BPF_CGROUP_INET_SOCK_CREATE: i32 = 2;
pub const BPF_CGROUP_SOCK_OPS: i32 = 3;
pub const BPF_SK_SKB_STREAM_PARSER: i32 = 4;
pub const BPF_SK_SKB_STREAM_VERDICT: i32 = 5;
pub const BPF_CGROUP_DEVICE: i32 = 6;
pub const BPF_SK_MSG_VERDICT: i32 = 7;
pub const BPF_CGROUP_INET4_BIND: i32 = 8;
pub const BPF_CGROUP_INET6_BIND: i32 = 9;
pub const BPF_CGROUP_INET4_CONNECT: i32 = 10;
pub const BPF_CGROUP_INET6_CONNECT: i32 = 11;
pub const BPF_CGROUP_INET4_POST_BIND: i32 = 12;
pub const BPF_CGROUP_INET6_POST_BIND: i32 = 13;
pub const BPF_CGROUP_UDP4_SENDMSG: i32 = 14;
pub const BPF_CGROUP_UDP6_SENDMSG: i32 = 15;
pub const BPF_LIRC_MODE2: i32 = 16;
pub const BPF_FLOW_DISSECTOR: i32 = 17;
pub const MAX_BPF_ATTACH_TYPE: i32 = BPF_FLOW_DISSECTOR + 1;

/// cgroup-bpf attach flags used in `BPF_PROG_ATTACH` command
///
/// NONE(default): No further bpf programs allowed in the subtree.
///
/// `BPF_F_ALLOW_OVERRIDE`: If a sub-cgroup installs some bpf program,
/// the program in this cgroup yields to sub-cgroup program.
///
/// `BPF_F_ALLOW_MULTI`: If a sub-cgroup installs some bpf program,
/// that cgroup program gets run in addition to the program in this cgroup.
///
/// Only one program is allowed to be attached to a cgroup with
/// NONE or `BPF_F_ALLOW_OVERRIDE` flag.
/// Attaching another program on top of NONE or `BPF_F_ALLOW_OVERRIDE` will
/// release old program and attach the new one. Attach flags has to match.
///
/// Multiple programs are allowed to be attached to a cgroup with
/// `BPF_F_ALLOW_MULTI` flag. They are executed in FIFO order
/// (those that were attached first, run first)
/// The programs of sub-cgroup are executed first, then programs of
/// this cgroup and then programs of parent cgroup.
/// When children program makes decision (like picking TCP CA or sock bind)
/// parent program has a chance to override it.
///
/// A cgroup with MULTI or OVERRIDE flag allows any attach flags in sub-cgroups.
/// A cgroup with NONE doesn't allow any programs in sub-cgroups.
/// Ex1:
/// cgrp1 (MULTI progs A, B) ->
///    cgrp2 (OVERRIDE prog C) ->
///      cgrp3 (MULTI prog D) ->
///        cgrp4 (OVERRIDE prog E) ->
///          cgrp5 (NONE prog F)
/// the event in cgrp5 triggers execution of F,D,A,B in that order.
/// if prog F is detached, the execution is E,D,A,B
/// if prog F and D are detached, the execution is E,A,B
/// if prog F, E and D are detached, the execution is C,A,B
///
/// All eligible programs are executed regardless of return code from
/// earlier programs.
pub const BPF_F_ALLOW_OVERRIDE: i32 = 1;
pub const BPF_F_ALLOW_MULTI: i32 = 1 << 1;

/// The verifier will perform strict alignment checking.
///
/// If `BPF_F_STRICT_ALIGNMENT` is used in `BPF_PROG_LOAD` command, as if
/// the kernel has been built with `CONFIG_EFFICIENT_UNALIGNED_ACCESS` not set,
/// and `NET_IP_ALIGN` defined to 2.
pub const BPF_F_STRICT_ALIGNMENT: i32 = 1;

/// If `BPF_F_ANY_ALIGNMENT` is used in `BPF_PROF_LOAD` command, the
/// verifier will allow any alignment whatsoever.
///
/// On platforms with strict alignment requirements for loads ands stores
/// (such as sparc and mips) the verifier validates that all loads and
/// stores provably follow this requirement.
/// This flag turns that checking and enforcement off.
///
/// It is mostly used for testing when we want to validate the
/// context and memory access aspects of the verifier, but because
/// of an unaligned access the alignment check would trigger before
/// the one we are interested in.
pub const BPF_F_ANY_ALIGNMENT: i32 = 1 << 1;

/// when `bpf_ldimm64->src_reg == BPF_PSEUDO_MAP_FD`, `bpf_ldimm64->imm == fd`
pub const BPF_PSEUDO_MAP_FD: i32 = 1;

/// when `bpf_call->src_reg == BPF_PSEUDO_CALL`, `bpf_call->imm == pc-relative`
/// offset to another bpf function
pub const BPF_PSEUDO_CALL: i32 = 1;

/// flags for `BPF_MAP_UPDATE_ELEM` command
/// create new element or update existing
pub const BPF_ANY: i32 = 0;
/// create new element if it didn't exist
pub const BPF_NOEXIST: i32 = 1;
/// update existing element
pub const BPF_EXIST: i32 = 2;
/// spin_lock-ed `map_lookup/map_update`
pub const BPF_F_LOCK: i32 = 4;

/// flags for `BPF_MAP_CREATE` command
pub const BPF_F_NO_PREALLOC: i32 = 1;

/// Instead of having one common LRU list in the `BPF_MAP_TYPE_LRU_[PERCPU_]` HASH map,
/// use a percpu LRU list which can scale and perform better.
///
/// Note, the LRU nodes (including free nodes) cannot be moved
/// across different LRU lists.
pub const BPF_F_NO_COMMON_LRU: i32 = 1 << 1;

/// Specify numa node during map creation
pub const BPF_F_NUMA_NODE: i32 = 1 << 2;

pub const BPF_OBJ_NAME_LEN: usize = 16;

/// Flags for accessing BPF object
pub const BPF_F_RDONLY: i32 = 1 << 3;
pub const BPF_F_WRONLY: i32 = 1 << 4;

/// Flag for `stack_map`, store `build_id+offset` instead of pointer
pub const BPF_F_STACK_BUILD_ID: i32 = 1 << 5;

/// Zero-initialize hash function seed. This should only be used for testing.
pub const BPF_F_ZERO_SEED: i32 = 1 << 6;

/// flags for `BPF_PROG_QUERY`
pub const BPF_F_QUERY_EFFECTIVE: i32 = 1;

/// user space need an empty entry to identify end of a trace
pub const BPF_STACK_BUILD_ID_EMPTY: i32 = 0;
/// with valid `build_id` and offset
pub const BPF_STACK_BUILD_ID_VALID: i32 = 1;
/// couldn't get `build_id`, fallback to ip
pub const BPF_STACK_BUILD_ID_IP: i32 = 2;

pub const BPF_BUILD_ID_SIZE: usize = 20;

#[repr(C)]
pub struct bpf_stack_build_id_t {
    pub status: i32,
    pub build_id: [u8; BPF_BUILD_ID_SIZE],

    /// Alias is `offset`
    pub ip: u64,
}

/// anonymous struct used by `BPF_MAP_CREATE` command
#[repr(C)]
#[derive(Clone, Copy)]
pub struct bpf_attr_map_create_t {
    /// one of enum `bpf_map_type`
    pub map_type: u32,
    /// size of key in bytes
    pub key_size: u32,
    /// size of value in bytes
    pub value_size: u32,
    /// max number of entries in a map
    pub max_entries: u32,
    /// `BPF_MAP_CREATE` related flags defined above.
    pub map_flags: u32,
    /// fd pointing to the inner map
    pub inner_map_fd: u32,
    /// numa node (effective only if `BPF_F_NUMA_NODE` is set).
    pub numa_node: u32,
    pub map_name: [u8; BPF_OBJ_NAME_LEN],
    /// ifindex of netdev to create on
    pub map_ifindex: u32,
    /// fd pointing to a BTF type data
    pub btf_fd: u32,
    /// BTF `type_id` of the key
    pub btf_key_type_id: u32,
    /// BTF `type_id` of the value
    pub btf_value_type_id: u32,
}

/// anonymous struct used by `BPF_MAP_*_ELEM` commands
#[repr(C)]
#[derive(Clone, Copy)]
pub struct bpf_attr_element_t {
    pub map_fd: u32,
    pub key: u64,

    /// Alias of value
    pub next_key: u64,
    pub flags: u64,
}

/// anonymous struct used by `BPF_PROG_LOAD` command
#[repr(C)]
#[derive(Clone, Copy)]
pub struct bpf_attr_prog_load_t {
    /// one of enum `bpf_prog_type`
    pub prog_type: u32,
    pub insn_cnt: u32,
    pub insns: u64,
    pub license: u64,
    /// verbosity level of verifier
    pub log_level: u32,
    /// size of user buffer
    pub log_size: u32,
    /// user supplied buffer
    pub log_buf: u64,
    /// not used
    pub kern_version: u32,
    pub prog_flags: u32,
    pub prog_name: [u8; BPF_OBJ_NAME_LEN],
    /// ifindex of netdev to prep for
    pub prog_ifindex: u32,
    /// For some prog types expected attach type must be known at
    /// load time to verify attach type specific parts of prog
    /// (context accesses, allowed helpers, etc).
    pub expected_attach_type: u32,
    /// fd pointing to BTF type data
    pub prog_btf_fd: u32,
    /// userspace `bpf_func_info` size
    pub func_info_rec_size: u32,
    /// func info
    pub func_info: u64,
    /// number of `bpf_func_info` records
    pub func_info_cnt: u32,
    /// userspace `bpf_line_info` size
    pub line_info_rec_size: u32,
    /// line info
    pub line_info: u64,
    /// number of `bpf_line_info` records
    pub line_info_cnt: u32,
}

/// anonymous struct used by `BPF_OBJ_*` commands
#[repr(C)]
#[derive(Clone, Copy)]
pub struct bpf_attr_obj_t {
    pub pathname: u64,
    pub bpf_fd: u32,
    pub file_flags: u32,
}

/// anonymous struct used by `BPF_PROG_ATTACH/DETACH` commands
#[repr(C)]
#[derive(Clone, Copy)]
pub struct bpf_attr_prog_attach_t {
    /// container object to attach to
    pub target_fd: u32,
    /// eBPF program to attach
    pub attach_bpf_fd: u32,
    pub attach_type: u32,
    pub attach_flags: u32,
}

/// anonymous struct used by `BPF_PROG_TEST_RUN` command
#[repr(C)]
#[derive(Clone, Copy)]
pub struct bpf_attr_prog_test_t {
    pub prog_fd: u32,
    pub retval: u32,
    /// input: len of `data_in`
    pub data_size_in: u32,
    /// input/output: len of `data_out` returns ENOSPC if `data_out` is too small.
    pub data_size_out: u32,
    pub data_in: u64,
    pub data_out: u64,
    pub repeat: u32,
    pub duration: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union bpf_attr_getid_id_t {
    pub start_id: u32,
    pub prog_id: u32,
    pub map_id: u32,
    pub btf_id: u32,
}

/// anonymous struct used by `BPF_*_GET_*_ID`
#[repr(C)]
#[derive(Clone, Copy)]
pub struct bpf_attr_getid_t {
    pub id: bpf_attr_getid_id_t,
    pub next_id: u32,
    pub open_flags: u32,
}

/// anonymous struct used by `BPF_OBJ_GET_INFO_BY_FD`
#[repr(C)]
#[derive(Clone, Copy)]
pub struct bpf_attr_info_t {
    pub bpf_fd: u32,
    pub info_len: u32,
    pub info: u64,
}

/// anonymous struct used by `BPF_PROG_QUERY` command
#[repr(C)]
#[derive(Clone, Copy)]
pub struct bpf_attr_query_t {
    /// container object to query
    pub target_fd: u32,
    pub attach_type: u32,
    pub query_flags: u32,
    pub attach_flags: u32,
    pub prog_ids: u64,
    pub prog_cnt: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct bpf_attr_raw_tracepoint_t {
    pub name: u64,
    pub prog_fd: u32,
}

/// anonymous struct for `BPF_BTF_LOAD`
#[repr(C)]
#[derive(Clone, Copy)]
pub struct bpf_attr_btf_load_t {
    pub btf: u64,
    pub btf_log_buf: u64,
    pub btf_size: u32,
    pub btf_log_size: u32,
    pub btf_log_level: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct bpf_attr_task_fd_query_t {
    /// input: pid
    pub pid: u32,
    /// input: fd
    pub fd: u32,
    /// input: flags
    pub flags: u32,
    /// input/output: buf len
    pub buf_len: u32,
    /// input/output:
    ///   `tp_name` for tracepoint symbol for kprobe filename for uprobe
    pub buf: u64,
    /// output: `prod_id`
    pub prog_id: u32,
    /// output: `BPF_FD_TYPE_*`
    pub fd_type: u32,
    /// output: `probe_offset`
    pub probe_offset: u64,
    /// output: `probe_addr`
    pub probe_addr: u64,
}

#[repr(C)]
pub union bpf_attr_t {
    pub map_create: bpf_attr_map_create_t,
    pub map_element: bpf_attr_element_t,
    pub prog_load: bpf_attr_prog_load_t,
    pub obj: bpf_attr_obj_t,
    pub prog_attach: bpf_attr_prog_attach_t,
    pub prog_test: bpf_attr_prog_test_t,
    pub getid: bpf_attr_getid_t,
    pub info: bpf_attr_info_t,
    pub query: bpf_attr_query_t,
    pub raw_tracepoint: bpf_attr_raw_tracepoint_t,
    pub btf_load: bpf_attr_btf_load_t,
    pub task_fd_query: bpf_attr_task_fd_query_t,
}

/* integer value in 'imm' field of BPF_CALL instruction selects which helper
 * function eBPF program intends to call
 */
//#define __BPF_ENUM_FN(x) BPF_FUNC_ ## x
//enum bpf_func_id {
//	__BPF_FUNC_MAPPER(__BPF_ENUM_FN)
//	__BPF_FUNC_MAX_ID,
//};
//#undef __BPF_ENUM_FN

// All flags used by eBPF helper functions, placed here.

/// `BPF_FUNC_skb_store_bytes` flags.
pub const BPF_F_RECOMPUTE_CSUM: i32 = 1;
pub const BPF_F_INVALIDATE_HASH: i32 = 1 << 1;

/// `BPF_FUNC_l3_csum_replace` and `BPF_FUNC_l4_csum_replace` flags.
///
/// First 4 bits are for passing the header field size.
pub const BPF_F_HDR_FIELD_MASK: i32 = 0xf;

/// `BPF_FUNC_l4_csum_replace` flags.
pub const BPF_F_PSEUDO_HDR: i32 = 1 << 4;
pub const BPF_F_MARK_MANGLED_0: i32 = 1 << 5;
pub const BPF_F_MARK_ENFORCE: i32 = 1 << 6;

/// `BPF_FUNC_clone_redirect` and `BPF_FUNC_redirect` flags.
pub const BPF_F_INGRESS: i32 = 1;

/// `BPF_FUNC_skb_set_tunnel_key` and `BPF_FUNC_skb_get_tunnel_key` flags.
pub const BPF_F_TUNINFO_IPV6: i32 = 1;

/// flags for both `BPF_FUNC_get_stackid` and `BPF_FUNC_get_stack`.
pub const BPF_F_SKIP_FIELD_MASK: i32 = 0xff;
pub const BPF_F_USER_STACK: i32 = 1 << 8;

/// flags used by `BPF_FUNC_get_stackid` only.
pub const BPF_F_FAST_STACK_CMP: i32 = 1 << 9;
pub const BPF_F_REUSE_STACKID: i32 = 1 << 10;
/// flags used by `BPF_FUNC_get_stack` only.
pub const BPF_F_USER_BUILD_ID: i32 = 1 << 11;

/// `BPF_FUNC_skb_set_tunnel_key` flags.
pub const BPF_F_ZERO_CSUM_TX: i32 = 1 << 1;
pub const BPF_F_DONT_FRAGMENT: i32 = 1 << 2;
pub const BPF_F_SEQ_NUMBER: i32 = 1 << 3;

/// `BPF_FUNC_perf_event_output`, `BPF_FUNC_perf_event_read` and
/// `BPF_FUNC_perf_event_read_value` flags.
pub const BPF_F_INDEX_MASK: u64 = 0xffff_ffff;
pub const BPF_F_CURRENT_CPU: u64 = BPF_F_INDEX_MASK;

/// `BPF_FUNC_perf_event_output` for `sk_buff` input context.
pub const BPF_F_CTXLEN_MASK: u64 = 0xfffff << 32;

/// Current network namespace
pub const BPF_F_CURRENT_NETNS: i32 = -1;

/// Mode for `BPF_FUNC_skb_adjust_room` helper.
pub const BPF_ADJ_ROOM_NET: i32 = 0;

/// Mode for `BPF_FUNC_skb_load_bytes_relative` helper.
pub const BPF_HDR_START_MAC: i32 = 0;
pub const BPF_HDR_START_NET: i32 = 1;

/// Encapsulation type for `BPF_FUNC_lwt_push_encap` helper.
pub const BPF_LWT_ENCAP_SEG6: i32 = 0;
pub const BPF_LWT_ENCAP_SEG6_INLINE: i32 = 1;
pub const BPF_LWT_ENCAP_IP: i32 = 2;

//#define __bpf_md_ptr(type, name)	\
//union {					\
//	type name;			\
//	__u64 :64;			\
//} __attribute__((aligned(8)))

/* user accessible mirror of in-kernel sk_buff.
 * new fields can only be added to the end of this structure
 */
//struct __sk_buff {
//	__u32 len;
//	__u32 pkt_type;
//	__u32 mark;
//	__u32 queue_mapping;
//	__u32 protocol;
//	__u32 vlan_present;
//	__u32 vlan_tci;
//	__u32 vlan_proto;
//	__u32 priority;
//	__u32 ingress_ifindex;
//	__u32 ifindex;
//	__u32 tc_index;
//	__u32 cb[5];
//	__u32 hash;
//	__u32 tc_classid;
//	__u32 data;
//	__u32 data_end;
//	__u32 napi_id;
//
//	/* Accessed by BPF_PROG_TYPE_sk_skb types from here to ... */
//	__u32 family;
//	__u32 remote_ip4;	/* Stored in network byte order */
//	__u32 local_ip4;	/* Stored in network byte order */
//	__u32 remote_ip6[4];	/* Stored in network byte order */
//	__u32 local_ip6[4];	/* Stored in network byte order */
//	__u32 remote_port;	/* Stored in network byte order */
//	__u32 local_port;	/* stored in host byte order */
//	/* ... here. */
//
//	__u32 data_meta;
//	__bpf_md_ptr(struct bpf_flow_keys *, flow_keys);
//	__u64 tstamp;
//	__u32 wire_len;
//	__u32 gso_segs;
//	__bpf_md_ptr(struct bpf_sock *, sk);
//};

//struct bpf_tunnel_key {
//	__u32 tunnel_id;
//	union {
//		__u32 remote_ipv4;
//		__u32 remote_ipv6[4];
//	};
//	__u8 tunnel_tos;
//	__u8 tunnel_ttl;
//	__u16 tunnel_ext;	/* Padding, future use. */
//	__u32 tunnel_label;
//};

/* user accessible mirror of in-kernel xfrm_state.
 * new fields can only be added to the end of this structure
 */
//struct bpf_xfrm_state {
//	__u32 reqid;
//	__u32 spi;	/* Stored in network byte order */
//	__u16 family;
//	__u16 ext;	/* Padding, future use. */
//	union {
//		__u32 remote_ipv4;	/* Stored in network byte order */
//		__u32 remote_ipv6[4];	/* Stored in network byte order */
//	};
//}

/// Generic BPF return codes which all BPF program types may support.
///
/// The values are binary compatible with their `TC_ACT_*` counter-part to
/// provide backwards compatibility with existing `SCHED_CLS` and `SCHED_ACT`
/// programs.
///
/// XDP is handled seprately, see XDP_*.
pub const BPF_OK: i32 = 0;
// 1 reserved
pub const BPF_DROP: i32 = 2;
// 3-6 reserved
pub const BPF_REDIRECT: i32 = 7;

/* >127 are reserved for prog type specific return codes.
 *
 * BPF_LWT_REROUTE: used by BPF_PROG_TYPE_LWT_IN and
 *    BPF_PROG_TYPE_LWT_XMIT to indicate that skb had been
 *    changed and should be routed based on its new L3 header.
 *    (This is an L3 redirect, as opposed to L2 redirect
 *    represented by BPF_REDIRECT above).
 */

pub const BPF_LWT_REROUTE: i32 = 128;

#[repr(C)]
pub struct bpf_sock_t {
    pub bound_dev_if: u32,
    pub family: u32,
    pub type_: u32,
    pub protocol: u32,
    pub mark: u32,
    pub priority: u32,
    /// IP address also allows 1 and 2 bytes access
    pub src_ip4: u32,
    pub src_ip6: [u32; 4],
    /// host byte order
    pub src_port: u32,
    /// network byte order
    pub dst_port: u32,
    pub dst_ip4: u32,
    pub dst_ip6: [u32; 4],
    pub state: u32,
}

#[repr(C)]
pub struct bpf_tcp_sock_t {
    /// Sending congestion window
    pub snd_cwnd: u32,
    /// smoothed round trip time << 3 in usecs
    pub srtt_us: u32,
    pub rtt_min: u32,
    /// Slow start size threshold
    pub snd_ssthresh: u32,
    /// What we want to receive next
    pub rcv_nxt: u32,
    /// Next sequence we send
    pub snd_nxt: u32,
    /// First byte we want an ack for
    pub snd_una: u32,
    /// Cached effective mss, not including SACKS
    pub mss_cache: u32,
    /// ECN status bits.
    pub ecn_flags: u32,
    /// saved rate sample: packets delivered
    pub rate_delivered: u32,
    /// saved rate sample: time elapsed
    pub rate_interval_us: u32,
    /// Packets which are "in flight"
    pub packets_out: u32,
    /// Retransmitted packets out
    pub retrans_out: u32,
    /// Total retransmits for entire connection
    pub total_retrans: u32,
    /// RFC4898 tcpEStatsPerfSegsIn total number of segments in
    pub segs_in: u32,
    /// RFC4898 tcpEStatsPerfDataSegsIn total number of data segments in.
    pub data_segs_in: u32,
    /// RFC4898 tcpEStatsPerfSegsOut The total number of segments sent.
    pub segs_out: u32,
    /// RFC4898 tcpEStatsPerfDataSegsOut total number of data segments sent.
    pub data_segs_out: u32,
    /// Lost packets
    pub lost_out: u32,
    /// SACK'd packets
    pub sacked_out: u32,
    /// RFC4898 tcpEStatsAppHCThruOctetsReceived
    /// `sum(delta(rcv_nxt`)), or how many bytes were acked.
    bytes_received: u64,
    /// RFC4898 tcpEStatsAppHCThruOctetsAcked
    /// `sum(delta(snd_una))`, or how many bytes were acked.
    pub bytes_acked: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct bpf_sock_tuple_ipv4_t {
    pub saddr: be32_t,
    pub daddr: be32_t,
    pub sport: be16_t,
    pub dport: be16_t,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct bpf_sock_tuple_ipv6_t {
    pub saddr: [be32_t; 4],
    pub daddr: [be32_t; 4],
    pub sport: be16_t,
    pub dport: be16_t,
}

#[repr(C)]
pub union bpf_sock_tuple_t {
    pub ipv4: bpf_sock_tuple_ipv4_t,
    pub ipv6: bpf_sock_tuple_ipv6_t,
}

pub const XDP_PACKET_HEADROOM: i32 = 256;

/// User return codes for XDP prog type.
///
/// A valid XDP program must return one of these defined values. All other
/// return codes are reserved for future use. Unknown return codes will
/// result in packet drops and a warning via `bpf_warn_invalid_xdp_action()`.
pub const XDP_ABORTED: i32 = 0;
pub const XDP_DROP: i32 = 1;
pub const XDP_PASS: i32 = 2;
pub const XDP_TX: i32 = 3;
pub const XDP_REDIRECT: i32 = 4;

/// user accessible metadata for XDP packet hook
/// new fields must be added to the end of this structure
#[repr(C)]
pub struct xdp_md_t {
    pub data: u32,
    pub data_end: u32,
    pub data_meta: u32,
    /// Below access go through struct `xdp_rxq_info`
    /// rxq->dev->ifindex
    pub ingress_ifindex: u32,
    /// rxq->queue_index  
    pub rx_queue_index: u32,
}

pub const SK_DROP: i32 = 0;
pub const SK_PASS: i32 = 1;

#[repr(C)]
pub union bpf_md_ptr_t {
    pub data: usize,
    _u64: u64,
}

#[repr(C)]
pub union bpf_md_end_ptr_t {
    pub data_end: usize,
    _u64: u64,
}

/// user accessible metadata for `SK_MSG` packet hook, new fields must
/// be added to the end of this structure
#[repr(C)]
pub struct sk_msg_md_t {
    pub data: bpf_md_ptr_t,
    pub data_end: bpf_md_end_ptr_t,

    pub family: u32,
    /// Stored in network byte order
    pub remote_ip4: u32,
    /// Stored in network byte order
    pub local_ip4: u32,
    /// Stored in network byte order
    pub remote_ip6: [u32; 4],
    /// Stored in network byte order
    pub local_ip6: [u32; 4],
    /// Stored in network byte order
    pub remote_port: u32,
    /// stored in host byte order
    pub local_port: u32,
    /// Total size of `sk_msg`
    pub size: u32,
}

#[repr(C)]
pub struct sk_reuseport_md_t {
    /// Start of directly accessible data. It begins from the tcp/udp header.
    pub data: bpf_md_ptr_t,
    /// End of directly accessible data
    pub data_end: bpf_md_end_ptr_t,

    /// Total length of packet (starting from the tcp/udp header).
    /// Note that the directly accessible bytes (`data_end - data`)
    /// could be less than this "len".  Those bytes could be
    /// indirectly read by a helper `bpf_skb_load_bytes()`.
    pub len: u32,

    /// Eth protocol in the mac header (network byte order). e.g.
    /// `ETH_P_IP(0x0800)` and `ETH_P_IPV6(0x86DD)`
    pub eth_protocol: u32,
    /// IP protocol. e.g. `IPPROTO_TCP`, `IPPROTO_UDP`
    pub ip_protocol: u32,
    /// Is sock bound to an INANY address?
    pub bind_inany: u32,
    /// A hash of the packet 4 tuples
    pub hash: u32,
}

pub const BPF_TAG_SIZE: usize = 8;

#[repr(C)]
pub struct bpf_prog_info_t {
    pub type_: u32,
    pub id: u32,
    pub tag: [u8; BPF_TAG_SIZE],
    pub jited_prog_len: u32,
    pub xlated_prog_len: u32,
    pub jited_prog_insns: u64,
    pub xlated_prog_insns: u64,
    /// ns since boottime
    pub load_time: u64,
    pub created_by_uid: u32,
    pub nr_map_ids: u32,
    pub map_ids: u64,
    pub name: [u8; BPF_OBJ_NAME_LEN],
    pub ifindex: u32,
    //__u32 gpl_compatible:1;
    //TOD(Shaohua): Fix type alignment error
    pub gpl_compatible: u8,
    pub netns_dev: u64,
    pub netns_ino: u64,
    pub nr_jited_ksyms: u32,
    pub nr_jited_func_lens: u32,
    pub jited_ksyms: u64,
    pub jited_func_lens: u64,
    pub btf_id: u32,
    pub func_info_rec_size: u32,
    pub func_info: u64,
    pub nr_func_info: u32,
    pub nr_line_info: u32,
    pub line_info: u64,
    pub jited_line_info: u64,
    pub nr_jited_line_info: u32,
    pub line_info_rec_size: u32,
    pub jited_line_info_rec_size: u32,
    pub nr_prog_tags: u32,
    pub prog_tags: u64,
    pub run_time_ns: u64,
    pub run_cnt: u64,
}

#[repr(C)]
pub struct bpf_map_info_t {
    pub type_: u32,
    pub id: u32,
    pub key_size: u32,
    pub value_size: u32,
    pub max_entries: u32,
    pub map_flags: u32,
    pub name: [u8; BPF_OBJ_NAME_LEN],
    pub ifindex: u32,
    //__u32 :32;
    pad: [u8; 32],
    pub netns_dev: u64,
    pub netns_ino: u64,
    pub btf_id: u32,
    pub btf_key_type_id: u32,
    pub btf_value_type_id: u32,
}

#[repr(C)]
pub struct bpf_btf_info_t {
    pub btf: u64,
    pub btf_size: u32,
    pub id: u32,
}

/// Use `bpf_sock_addr` struct to access socket fields and sockaddr struct passed
/// by user and intended to be used by socket (e.g. to bind to, depends on
/// attach attach type).
#[repr(C)]
pub struct bpf_sock_addr_t {
    /// Allows 4-byte read, but no write.
    pub user_family: u32,

    /// Allows 1,2,4-byte read and 4-byte write.  Stored in network byte order.
    pub user_ip4: u32,

    /// Allows 1,2,4-byte read an 4-byte write.  Stored in network byte order.
    pub user_ip6: [u32; 4],

    /// Allows 4-byte read and write.  Stored in network byte order
    pub user_port: u32,

    /// Allows 4-byte read, but no write
    pub family: u32,

    /// Allows 4-byte read, but no write
    pub type_: u32,

    /// Allows 4-byte read, but no write
    pub protocol: u32,

    /// Allows 1,2,4-byte read an 4-byte write. Stored in network byte order.
    pub msg_src_ip4: u32,

    /// Allows 1,2,4-byte read an 4-byte write.  Stored in network byte order.
    pub msg_src_ip6: [u32; 4],
}

#[repr(C)]
pub union bpf_sock_ops_reply_t {
    /// Optionally passed to bpf program
    pub args: [u32; 4],

    /// Returned by bpf program
    pub reply: u32,

    /* Optionally returned by bpf prog  */
    pub replylong: [u32; 4],
}

/// User `bpf_sock_ops` struct to access socket values and specify request ops
/// and their replies.
///
/// Some of this fields are in network (bigendian) byte order and may need
/// to be converted before use (`bpf_ntohl()` defined in `samples/bpf/bpf_endian.h`).
/// New fields can only be added at the end of this structure
#[repr(C)]
pub struct bpf_sock_ops_t {
    pub op: u32,
    pub reply: bpf_sock_ops_reply_t,

    pub family: u32,

    /// Stored in network byte order
    pub remote_ip4: u32,

    /// Stored in network byte order
    pub local_ip4: u32,

    /// Stored in network byte order
    pub remote_ip6: [u32; 4],

    /// Stored in network byte order
    pub local_ip6: [u32; 4],

    /// Stored in network byte order
    pub remote_port: u32,

    /// stored in host byte order
    pub local_port: u32,

    /// Some TCP fields are only valid if there is a full socket.
    /// If not, the fields read as zero.
    pub is_fullsock: u32,

    pub snd_cwnd: u32,

    /// Averaged RTT << 3 in usecs
    pub srtt_us: u32,

    /// flags defined in uapi/linux/tcp.h
    pub bpf_sock_ops_cb_flags: u32,

    pub state: u32,
    pub rtt_min: u32,
    pub snd_ssthresh: u32,
    pub rcv_nxt: u32,
    pub snd_nxt: u32,
    pub snd_una: u32,
    pub mss_cache: u32,
    pub ecn_flags: u32,
    pub rate_delivered: u32,
    pub rate_interval_us: u32,
    pub packets_out: u32,
    pub retrans_out: u32,
    pub total_retrans: u32,
    pub segs_in: u32,
    pub data_segs_in: u32,
    pub segs_out: u32,
    pub data_segs_out: u32,
    pub lost_out: u32,
    pub sacked_out: u32,
    pub sk_txhash: u32,
    pub bytes_received: u64,
    pub bytes_acked: u64,
}

/// Definitions for `bpf_sock_ops_cb_flags`
pub const BPF_SOCK_OPS_RTO_CB_FLAG: i32 = 1;
pub const BPF_SOCK_OPS_RETRANS_CB_FLAG: i32 = 1 << 1;
pub const BPF_SOCK_OPS_STATE_CB_FLAG: i32 = 1 << 2;
/// Mask of all currently supported cb flags
pub const BPF_SOCK_OPS_ALL_CB_FLAGS: i32 = 0x7;

/// List of known BPF `sock_ops` operators.
/// New entries can only be added at the end
pub const BPF_SOCK_OPS_VOID: i32 = 0;

/// Should return SYN-RTO value to use or -1 if default value should be used
pub const BPF_SOCK_OPS_TIMEOUT_INIT: i32 = 1;

/// Should return initial advertized window (in packets) or -1 if default
/// value should be used
pub const BPF_SOCK_OPS_RWND_INIT: i32 = 2;

/// Calls BPF program right before an active connection is initialized
pub const BPF_SOCK_OPS_TCP_CONNECT_CB: i32 = 3;

/// Calls BPF program when an active connection is established
pub const BPF_SOCK_OPS_ACTIVE_ESTABLISHED_CB: i32 = 4;

/// Calls BPF program when a passive connection is established
pub const BPF_SOCK_OPS_PASSIVE_ESTABLISHED_CB: i32 = 5;

/// If connection's congestion control needs ECN
pub const BPF_SOCK_OPS_NEEDS_ECN: i32 = 6;

/// Get base RTT.
///
/// The correct value is based on the path and may be dependent
/// on the congestion control algorithm.
/// In general it indicates a congestion threshold.
/// RTTs above this indicate congestion
pub const BPF_SOCK_OPS_BASE_RTT: i32 = 7;

/// Called when an RTO has triggered.
///
/// Arg1: value of `icsk_retransmits`
/// Arg2: value of `icsk_rto`
/// Arg3: whether RTO has expired
pub const BPF_SOCK_OPS_RTO_CB: i32 = 8;

/// Called when skb is retransmitted.
///
/// Arg1: sequence number of 1st byte
/// Arg2: # segments
/// Arg3: return value of `tcp_transmit_skb` (0 => success)
pub const BPF_SOCK_OPS_RETRANS_CB: i32 = 9;

/// Called when TCP changes state.
///
/// Arg1: `old_state`
/// Arg2: `new_state`
pub const BPF_SOCK_OPS_STATE_CB: i32 = 10;

/// Called on listen(2), right after socket transition to LISTEN state.
pub const BPF_SOCK_OPS_TCP_LISTEN_CB: i32 = 11;

/// List of TCP states.
///
/// There is a build check in net/ipv4/tcp.c to detect changes between the TCP
/// and BPF versions.
/// Ideally this should never happen.
/// If it does, we need to add code to convert them before calling
/// the BPF `sock_ops` function.
pub const BPF_TCP_ESTABLISHED: i32 = 1;
pub const BPF_TCP_SYN_SENT: i32 = 2;
pub const BPF_TCP_SYN_RECV: i32 = 3;
pub const BPF_TCP_FIN_WAIT1: i32 = 4;
pub const BPF_TCP_FIN_WAIT2: i32 = 5;
pub const BPF_TCP_TIME_WAIT: i32 = 6;
pub const BPF_TCP_CLOSE: i32 = 7;
pub const BPF_TCP_CLOSE_WAIT: i32 = 8;
pub const BPF_TCP_LAST_ACK: i32 = 9;
pub const BPF_TCP_LISTEN: i32 = 10;
/// Now a valid state
pub const BPF_TCP_CLOSING: i32 = 11;
pub const BPF_TCP_NEW_SYN_RECV: i32 = 12;
/// Leave at the end!
pub const BPF_TCP_MAX_STATES: i32 = 13;

/// Set TCP initial congestion window
pub const TCP_BPF_IW: i32 = 1001;
/// Set `sndcwnd_clamp`
pub const TCP_BPF_SNDCWND_CLAMP: i32 = 1002;

#[repr(C)]
pub struct bpf_perf_event_value_t {
    pub counter: u64,
    pub enabled: u64,
    pub running: u64,
}

pub const BPF_DEVCG_ACC_MKNOD: i32 = 1;
pub const BPF_DEVCG_ACC_READ: i32 = 1 << 1;
pub const BPF_DEVCG_ACC_WRITE: i32 = 1 << 2;

pub const BPF_DEVCG_DEV_BLOCK: i32 = 1;
pub const BPF_DEVCG_DEV_CHAR: i32 = 1 << 1;

#[repr(C)]
pub struct bpf_cgroup_dev_ctx_t {
    /// `access_type` encoded as `(BPF_DEVCG_ACC_* << 16) | BPF_DEVCG_DEV_*`
    pub access_type: u32,
    pub major: u32,
    pub minor: u32,
}

#[repr(C)]
pub struct bpf_raw_tracepoint_args_t {
    pub args: [u64; 0],
}

/// DIRECT:  Skip the FIB rules and go to FIB table associated with device
/// OUTPUT:  Do lookup from egress perspective; default is ingress
pub const BPF_FIB_LOOKUP_DIRECT: i32 = 0;
pub const BPF_FIB_LOOKUP_OUTPUT: i32 = 1;

/// lookup successful
pub const BPF_FIB_LKUP_RET_SUCCESS: i32 = 0;
/// dest is blackholed; can be dropped
pub const BPF_FIB_LKUP_RET_BLACKHOLE: i32 = 1;
/// dest is unreachable; can be dropped
pub const BPF_FIB_LKUP_RET_UNREACHABLE: i32 = 2;
/// dest not allowed; can be dropped
pub const BPF_FIB_LKUP_RET_PROHIBIT: i32 = 3;
/// packet is not forwarded
pub const BPF_FIB_LKUP_RET_NOT_FWDED: i32 = 4;
/// fwding is not enabled on ingress
pub const BPF_FIB_LKUP_RET_FWD_DISABLED: i32 = 5;
/// fwd requires encapsulation
pub const BPF_FIB_LKUP_RET_UNSUPP_LWT: i32 = 6;
/// no neighbor entry for nh
pub const BPF_FIB_LKUP_RET_NO_NEIGH: i32 = 7;
/// fragmentation required to fwd
pub const BPF_FIB_LKUP_RET_FRAG_NEEDED: i32 = 8;

/// inputs to lookup
#[repr(C)]
pub union bpf_fib_lookup_inputs_t {
    /// `AF_INET`
    pub tos: u8,

    /// `AF_INET6`, `flow_label + priority`
    pub flowinfo: be32_t,

    /// output: metric of fib result (IPv4/IPv6 only)
    pub rt_metric: u32,
}

#[repr(C)]
pub union bpf_fib_lookup_addr_t {
    pub ipv4: be32_t,

    /// `in6_addr`; network order
    pub ipv6: [u32; 4],
}

#[repr(C)]
pub struct bpf_fib_lookup_t {
    /// input:  network family for lookup (`AF_INET`, `AF_INET6`)
    /// output: network family of egress nexthop
    pub family: u8,

    /// set if lookup is to consider L4 data - e.g., FIB rules
    pub l4_protocol: u8,
    pub sport: be16_t,
    pub dport: be16_t,

    /// total length of packet from network header - used for MTU check
    pub tot_len: u16,

    /// input: L3 device index for lookup
    /// output: device index from FIB lookup
    pub ifindex: u32,

    pub inputs: bpf_fib_lookup_inputs_t,

    pub src: bpf_fib_lookup_addr_t,

    /// input to `bpf_fib_lookup`, `ipv{4,6}_dst` is destination address in
    /// network header. output: `bpf_fib_lookup` sets to gateway address
    /// if FIB lookup returns gateway route
    pub dest: bpf_fib_lookup_addr_t,

    /// output
    pub h_vlan_proto: be16_t,
    pub h_vlan_tci: be16_t,
    /// `ETH_ALEN`
    pub smac: [u8; 6],
    /// `ETH_ALEN`
    pub dmac: [u8; 6],
}

/// tp name
pub const BPF_FD_TYPE_RAW_TRACEPOINT: i32 = 0;
/// tp name
pub const BPF_FD_TYPE_TRACEPOINT: i32 = 1;
/// `(symbol + offset)` or addr
pub const BPF_FD_TYPE_KPROBE: i32 = 3;
/// `(symbol + offset)` or addr
pub const BPF_FD_TYPE_KRETPROBE: i32 = 4;
/// `filename + offset`
pub const BPF_FD_TYPE_UPROBE: i32 = 5;
/// `filename + offset`
pub const BPF_FD_TYPE_URETPROBE: i32 = 6;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct bpf_flow_keys_ipv4_t {
    pub ipv4_src: be32_t,
    pub ipv4_dst: be32_t,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct bpf_flow_keys_ipv6_t {
    // TODO(Shaohua): use be32_t
    /// `in6_addr`; network order
    pub ipv6_src: [u32; 4],
    /// `in6_addr`; network order */
    pub ipv6_dst: [u32; 4],
}

#[repr(C)]
pub union bpf_flow_keys_addr_t {
    pub ipv4: bpf_flow_keys_ipv4_t,
    pub ipv6: bpf_flow_keys_ipv6_t,
}

#[repr(C)]
pub struct bpf_flow_keys_t {
    pub nhoff: u16,
    pub thoff: u16,
    /// `ETH_P_*` of valid addrs
    pub addr_proto: u16,
    pub is_frag: u8,
    pub is_first_frag: u8,
    pub is_encap: u8,
    pub ip_proto: u8,
    pub n_proto: be16_t,
    pub sport: be16_t,
    pub dport: be16_t,
    pub addr: bpf_flow_keys_addr_t,
}

#[repr(C)]
pub struct bpf_func_info_t {
    pub insn_off: u32,
    pub type_id: u32,
}

// TODO(Shaohua):
//#define BPF_LINE_INFO_LINE_NUM(line_col)	((line_col) >> 10)
//#define BPF_LINE_INFO_LINE_COL(line_col)	((line_col) & 0x3ff)

#[repr(C)]
pub struct bpf_line_info_t {
    pub insn_off: u32,
    pub file_name_off: u32,
    pub line_off: u32,
    pub line_col: u32,
}

#[repr(C)]
pub struct bpf_spin_lock_t {
    val: u32,
}
