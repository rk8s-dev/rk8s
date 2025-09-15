// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

use std::ffi::CString;

pub fn set_process_name(name: &str) -> Result<(), nc::Errno> {
    let process_name = CString::new(name).unwrap();
    let name_ptr = process_name.as_ptr() as usize;
    unsafe { nc::prctl(nc::PR_SET_NAME, name_ptr, 0, 0, 0).map(|_ret| ()) }
}

fn main() {
    let process_name = "rust-001";
    let pid = unsafe { nc::getpid() };
    println!("pid: {}, process name: {}", pid, &process_name);
    set_process_name(process_name).unwrap();

    let mask = nc::sigset_t::default();
    let _ret = unsafe { nc::rt_sigsuspend(&mask) };
}
