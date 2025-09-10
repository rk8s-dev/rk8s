// Copyright (c) 2024 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

fn call_ls() {
    println!("[ls] pid: {}", unsafe { nc::getpid() });
    let args = ["ls", "-l", "-a"];
    let env = ["DISPLAY=wayland"];
    let ret = unsafe { nc::execve("/bin/ls", &args, &env) };
    assert!(ret.is_ok());
}

fn call_date() {
    println!("[date] pid: {}", unsafe { nc::getpid() });
    let args = ["date", "+%Y:%m:%d %H:%M:%S"];
    let env = ["DISPLAY=:0"];
    let ret = unsafe { nc::execveat(nc::AT_FDCWD, "/bin/date", &args, &env, 0) };
    assert!(ret.is_ok());
}

fn call_id() {
    let ret = unsafe { nc::openat(nc::AT_FDCWD, "/usr/bin/id", nc::O_RDONLY | nc::O_CLOEXEC, 0) };
    assert!(ret.is_ok());
    let fd = ret.unwrap();
    let args = ["id", "-u", "-n"];
    let env = ["DISPLAY=:0"];
    let ret = unsafe { nc::execveat(fd, "", &args, &env, nc::AT_EMPTY_PATH) };
    assert!(ret.is_ok());
}

fn main() {
    let pid = unsafe { nc::fork() };
    match pid {
        Err(errno) => eprintln!("Failed to call fork(), err: {errno}"),
        Ok(0) => {
            // Child process
            call_ls();
        }
        Ok(child_pid) => {
            // Parent process
            println!("[main] child pid is: {child_pid}");
        }
    }

    let pid = unsafe { nc::fork() };
    match pid {
        Err(errno) => eprintln!("Failed to call fork(), err: {errno}"),
        Ok(0) => {
            // Child process
            call_date();
        }
        Ok(child_pid) => {
            // Parent process
            println!("[main] child pid is: {child_pid}");
        }
    }

    let pid = unsafe { nc::fork() };
    match pid {
        Err(errno) => eprintln!("Failed to call fork(), err: {errno}"),
        Ok(0) => {
            // Child process
            call_id();
        }
        Ok(child_pid) => {
            // Parent process
            println!("[main] child pid is: {child_pid}");
        }
    }

    // Wait for a while.
    let ts = nc::timespec_t {
        tv_sec: 2,
        tv_nsec: 0,
    };
    unsafe {
        let _ret = nc::nanosleep(&ts, None);
    }
}
