// Copyright (c) 2024 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

fn signal_handler(sig_num: i32) {
    println!("signal handler {sig_num}");
}

const SIGNALS: [i32; 14] = [
    nc::SIGSEGV,
    nc::SIGHUP,
    nc::SIGINT,
    nc::SIGQUIT,
    nc::SIGILL,
    nc::SIGABRT,
    nc::SIGBUS,
    nc::SIGFPE,
    nc::SIGUSR1,
    nc::SIGUSR2,
    nc::SIGPIPE,
    nc::SIGALRM,
    nc::SIGTERM,
    nc::SIGCHLD,
];

fn main() {
    let sa = nc::new_sigaction(signal_handler);
    for sig_num in SIGNALS {
        let ret = unsafe { nc::rt_sigaction(sig_num, Some(&sa), None) };
        assert!(ret.is_ok());
        println!("register signal handler for {sig_num}");
    }

    let pid = unsafe { nc::getpid() };
    println!("pid: {}", pid);

    loop {
        let t = nc::timespec_t {
            tv_sec: 10,
            tv_nsec: 0,
        };
        unsafe {
            let _ret = nc::nanosleep(&t, None);
        }
    }
}
