// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

fn main() {
    let mut buf = [0_u8; nc::PATH_MAX as usize + 1];
    let ret = unsafe { nc::getcwd(&mut buf) };
    assert!(ret.is_ok());
    // Remove null-terminal char.
    let path_len = ret.unwrap() as usize - 1;
    let cwd = std::str::from_utf8(&buf[..path_len]);
    assert!(cwd.is_ok());
    println!("cwd: {:?}", cwd.unwrap());
}
