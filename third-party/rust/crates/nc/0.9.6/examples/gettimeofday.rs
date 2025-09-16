// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

fn main() {
    let mut timeval = nc::timeval_t::default();
    let ret = unsafe { nc::gettimeofday(&mut timeval, None) };
    match ret {
        Err(errno) => eprintln!("gettimeofday() failed: {}", nc::strerror(errno)),
        Ok(_) => println!("time: {:+?}", timeval),
    }
}
