// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

fn main() {
    unsafe { nc::setpriority(nc::PRIO_PROCESS, 0, -5).expect("Failed to set priority to -5!") };
    let prio = unsafe { nc::getpriority(nc::PRIO_PROCESS, 0) };
    println!("prio: {:?}", prio);
}
