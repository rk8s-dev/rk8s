// Copyright (c) 2023 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `sys/_ffcounter.h`

/// The feed-forward clock counter. The fundamental element of a feed-forward
/// clock is a wide monotonically increasing counter that accumulates at the same
/// rate as the selected timecounter.
pub type ffcounter_t = u64;
