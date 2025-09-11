// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `amd64/include/syarch.h`

pub const I386_GET_LDT: i32 = 0;
pub const I386_SET_LDT: i32 = 1;
#[allow(overflowing_literals)]
pub const LDT_AUTO_ALLOC: i32 = 0xffffffff;
/// I386_IOPL
pub const I386_GET_IOPERM: i32 = 3;
pub const I386_SET_IOPERM: i32 = 4;
// NOTE: Not implementable on amd64
pub const I386_VM86: i32 = 6;
pub const I386_GET_FSBASE: i32 = 7;
pub const I386_SET_FSBASE: i32 = 8;
pub const I386_GET_GSBASE: i32 = 9;
pub const I386_SET_GSBASE: i32 = 10;
pub const I386_GET_XFPUSTATE: i32 = 11;
pub const I386_SET_PKRU: i32 = 12;
pub const I386_CLEAR_PKRU: i32 = 13;

/// Leave space for 0-127 for to avoid translating syscalls
pub const AMD64_GET_FSBASE: i32 = 128;
pub const AMD64_SET_FSBASE: i32 = 129;
pub const AMD64_GET_GSBASE: i32 = 130;
pub const AMD64_SET_GSBASE: i32 = 131;
pub const AMD64_GET_XFPUSTATE: i32 = 132;
pub const AMD64_SET_PKRU: i32 = 133;
pub const AMD64_CLEAR_PKRU: i32 = 134;

/// Flags for AMD64_SET_PKRU
pub const AMD64_PKRU_EXCL: i32 = 0x0001;
pub const AMD64_PKRU_PERSIST: i32 = 0x0002;

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct i386_ioperm_args_t {
    pub start: u32,
    pub length: u32,
    pub enable: i32,
}

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct i386_ldt_args_t {
    pub start: u32,
    //pub descs: *mut user_segment_descriptor_t,
    pub descs: usize,
    pub num: u32,
}

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct i386_get_xfpustate_t {
    pub addr: u32,
    pub len: i32,
}

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct i386_set_pkru_t {
    pub addr: u32,
    pub len: u32,
    pub keyidx: u32,
    pub flags: i32,
}

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct amd64_get_xfpustate_t {
    pub addr: usize,
    pub len: i32,
}

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct amd64_set_pkru_t {
    pub addr: usize,
    pub len: usize,
    pub keyidx: u32,
    pub flags: i32,
}
