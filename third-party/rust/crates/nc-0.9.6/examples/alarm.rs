// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

#[no_mangle]
fn handle_alarm(signum: i32) {
    println!("handle alarm");
    assert_eq!(signum, nc::SIGALRM);
}

fn main() {
    let sa = nc::new_sigaction(handle_alarm);
    let ret = unsafe { nc::rt_sigaction(nc::SIGALRM, Some(&sa), None) };
    assert!(ret.is_ok());

    let seconds = 1;
    let remaining = unsafe { nc::alarm(seconds) };
    let mask = nc::sigset_t::default();
    let ret = unsafe { nc::rt_sigsuspend(&mask) };
    assert!(ret.is_err());
    assert_eq!(ret, Err(nc::EINTR));
    assert_eq!(remaining.unwrap(), 0);
}
