// Copyright (c) 2024 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

fn main() {
    fn handle_alarm(signum: i32) {
        assert_eq!(signum, nc::SIGALRM);
        let msg = "Hello alarm\n";
        let _ = unsafe { nc::write(2, msg.as_bytes()) };
    }

    let sa = nc::new_sigaction(handle_alarm);
    let ret = unsafe { nc::rt_sigaction(nc::SIGALRM, Some(&sa), None) };
    assert!(ret.is_ok());

    // Single shot timer, actived after 1 second.
    let itv = nc::itimerval_t {
        it_value: nc::timeval_t {
            tv_sec: 1,
            tv_usec: 0,
        },
        it_interval: nc::timeval_t {
            tv_sec: 0,
            tv_usec: 0,
        },
    };
    let ret = unsafe { nc::setitimer(nc::ITIMER_REAL, &itv, None) };
    assert!(ret.is_ok());

    let mut prev_itv = nc::itimerval_t::default();
    let ret = unsafe { nc::getitimer(nc::ITIMER_REAL, &mut prev_itv) };
    assert!(ret.is_ok());
    assert!(prev_itv.it_value.tv_sec <= itv.it_value.tv_sec);

    let mask = nc::sigset_t::default();
    let ret = unsafe { nc::rt_sigsuspend(&mask) };
    assert!(ret.is_err());

    let ret = unsafe { nc::getitimer(nc::ITIMER_REAL, &mut prev_itv) };
    assert!(ret.is_ok());
    assert_eq!(prev_itv.it_value.tv_sec, 0);
    assert_eq!(prev_itv.it_value.tv_usec, 0);
}
