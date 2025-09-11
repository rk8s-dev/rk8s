// Copyright (c) 2024 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

fn main() {
    let pid = unsafe { nc::fork() };
    match pid {
        Err(errno) => eprintln!("Failed to call fork(), err: {}", nc::strerror(errno)),
        Ok(0) => {
            // Child process
            println!("[child] pid: {}", unsafe { nc::getpid() });
            let args = ["ls", "-l", "-a"];
            let env = ["DISPLAY=wayland"];
            let ret = unsafe { nc::execve("/bin/ls", &args, &env) };
            assert!(ret.is_ok());
        }
        Ok(child_pid) => {
            // Parent process
            println!("[main] child pid is: {child_pid}");
        }
    }
}
