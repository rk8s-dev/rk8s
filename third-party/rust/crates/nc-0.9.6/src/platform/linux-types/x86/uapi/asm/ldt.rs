// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

/// From `arch/x86/include/uapi/asm/ldt.h`
///
/// Definitions of structures used with the `modify_ldt` system call.

/// Maximum number of LDT entries supported.
pub const LDT_ENTRIES: i32 = 8192;
/// The size of each LDT entry.
pub const LDT_ENTRY_SIZE: i32 = 8;

/// Note on 64bit base and limit is ignored and you cannot set DS/ES/CS
/// not to the default values if you still want to do syscalls. This
/// call is more for 32bit mode therefore.
#[repr(C)]
pub struct user_desc_t {
    pub entry_number: u32,
    pub base_addr: u32,
    pub limit: u32,
    //pub seg_32bit: 1,
    pub seg_32bit: u8,
    //pub contents: 2,
    pub contents: u8,
    //pub read_exec_only: 1,
    pub read_exec_only: u8,
    //pub limit_in_pages: 1,
    pub limit_in_pages: u8,
    //pub seg_not_present: 1,
    pub seg_not_present: u8,
    //pub useable: 1,
    pub useable: u8,
}

pub const MODIFY_LDT_CONTENTS_DATA: i32 = 0;
pub const MODIFY_LDT_CONTENTS_STACK: i32 = 1;
pub const MODIFY_LDT_CONTENTS_CODE: i32 = 2;
