// Copyright (c) 2024 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::similar_names)]
#![allow(clippy::wildcard_imports)]

extern crate alloc;
use alloc::vec::Vec;

use crate::c_str::CString;
use crate::path::Path;
use crate::syscalls::*;
use crate::sysno::*;
use crate::types::*;
