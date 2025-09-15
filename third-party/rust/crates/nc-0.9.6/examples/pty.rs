// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

fn ptsname(pty_fd: i32) -> Result<String, nc::Errno> {
    #[allow(unused_mut)]
    let mut n: i32 = 0;
    let n_ptr = (&mut n) as *mut i32 as *const _;
    unsafe { nc::ioctl(pty_fd, nc::TIOCGPTN, n_ptr)? };
    Ok(format!("/dev/pts/{}", n))
}

fn unlockpt(pty_fd: i32) -> Result<i32, nc::Errno> {
    let u: i32 = 0;
    let u_ptr = &u as *const i32 as *const _;
    unsafe { nc::ioctl(pty_fd, nc::TIOCSPTLCK, u_ptr) }
}

fn open_pty() -> Result<(i32, i32), nc::Errno> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    let pty_fd = unsafe { nc::openat(nc::AT_FDCWD, "/dev/ptmx", nc::O_RDWR, 0)? };
    #[cfg(target_os = "freebsd")]
    let pty_fd = unsafe { nc::open("/dev/ptmx", nc::O_RDWR, 0)? };

    println!("pty_fd: {}", pty_fd);

    let sname = ptsname(pty_fd)?;
    println!("sname: {}", sname);
    unlockpt(pty_fd)?;

    #[cfg(any(target_os = "linux", target_os = "android"))]
    let tty_fd = unsafe { nc::openat(nc::AT_FDCWD, &sname, nc::O_RDWR | nc::O_NOCTTY, 0)? };

    #[cfg(target_os = "freebsd")]
    let tty_fd = unsafe { nc::open(&sname, nc::O_RDWR | nc::O_NOCTTY, 0)? };

    println!("tty_fd: {}", tty_fd);
    Ok((pty_fd, tty_fd))
}

fn main() {
    if let Ok((pty, tty)) = open_pty() {
        println!("pty: {}, tty: {}", pty, tty);
    }
}
