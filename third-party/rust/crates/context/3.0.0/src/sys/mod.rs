// Copyright 2016 coroutine-rs Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use std::cmp;

#[cfg(unix)]
mod unix;

#[cfg(unix)]
pub use self::unix::{
    allocate_stack,
    deallocate_stack,
    max_stack_size,
    min_stack_size,
    page_size,
    protect_stack,
};

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use self::windows::{
    allocate_stack,
    deallocate_stack,
    max_stack_size,
    min_stack_size,
    page_size,
    protect_stack,
};

pub fn default_stack_size() -> usize {
    let size = self::min_stack_size() * 8;
    let max_stack_size = self::max_stack_size();

    cmp::min(size, max_stack_size)
}
