// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

fn main() {
    let mut info = nc::sysinfo_t::default();
    let _ = unsafe { nc::sysinfo(&mut info) };

    println!("free mem: {}", info.freeram * info.mem_unit as usize);
    println!("total mem: {}", info.totalram * info.mem_unit as usize);
}
