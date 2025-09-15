// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/_types/_id_t.h`

/// can hold `pid_t`, `gid_t`, or `uid_t`
pub type id_t = u32;
