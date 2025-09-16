// Copyright (c) 2021 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `amd64/inclue/pmap.h`
//!
//! Only part of header file is imported.

pub type pd_entry_t = u64;
pub type pt_entry_t = u64;
pub type pdp_entry_t = u64;
pub type pml4_entry_t = u64;
pub type pml5_entry_t = u64;
