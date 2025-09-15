// Copyright 2016 coroutine-rs Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use std::io;
use std::mem;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::usize;

use winapi;

use c_void;
use stack::Stack;

pub unsafe fn allocate_stack(size: usize) -> io::Result<Stack> {
    const NULL: winapi::shared::minwindef::LPVOID = 0 as winapi::shared::minwindef::LPVOID;
    const PROT: winapi::shared::minwindef::DWORD = winapi::um::winnt::PAGE_READWRITE;
    const TYPE: winapi::shared::minwindef::DWORD =
        winapi::um::winnt::MEM_COMMIT | winapi::um::winnt::MEM_RESERVE;

    let ptr = winapi::um::memoryapi::VirtualAlloc(
        NULL,
        size as winapi::shared::basetsd::SIZE_T,
        TYPE,
        PROT,
    );

    if ptr == NULL {
        Err(io::Error::last_os_error())
    } else {
        Ok(Stack::new(
            (ptr as usize + size) as *mut c_void,
            ptr as *mut c_void,
        ))
    }
}

pub unsafe fn protect_stack(stack: &Stack) -> io::Result<Stack> {
    const TYPE: winapi::shared::minwindef::DWORD =
        winapi::um::winnt::PAGE_READWRITE | winapi::um::winnt::PAGE_GUARD;

    let page_size = page_size();
    let mut old_prot: winapi::shared::minwindef::DWORD = 0;

    debug_assert!(stack.len() % page_size == 0 && stack.len() != 0);

    let ret = {
        let page_size = page_size as winapi::shared::basetsd::SIZE_T;
        winapi::um::memoryapi::VirtualProtect(stack.bottom(), page_size, TYPE, &mut old_prot)
    };

    if ret == 0 {
        Err(io::Error::last_os_error())
    } else {
        let bottom = (stack.bottom() as usize + page_size) as *mut c_void;
        Ok(Stack::new(stack.top(), bottom))
    }
}

pub unsafe fn deallocate_stack(ptr: *mut c_void, _: usize) {
    winapi::um::memoryapi::VirtualFree(
        ptr as winapi::shared::minwindef::LPVOID,
        0,
        winapi::um::winnt::MEM_RELEASE,
    );
}

pub fn page_size() -> usize {
    static PAGE_SIZE: AtomicUsize = AtomicUsize::new(0);

    let mut ret = PAGE_SIZE.load(Ordering::Relaxed);

    if ret == 0 {
        ret = unsafe {
            let mut info = mem::zeroed();
            winapi::um::sysinfoapi::GetSystemInfo(&mut info);
            info.dwPageSize as usize
        };

        PAGE_SIZE.store(ret, Ordering::Relaxed);
    }

    ret
}

// Windows does not seem to provide a stack limit API
pub fn min_stack_size() -> usize {
    page_size()
}

// Windows does not seem to provide a stack limit API
pub fn max_stack_size() -> usize {
    usize::MAX
}
