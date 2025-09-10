// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

fn main() {
    let msg = "hello, world\n";
    let nwrite = unsafe { nc::write(-1, msg.as_bytes()) };
    match nwrite {
        Ok(n) => {
            println!("nwrite: {}", n);
        }
        Err(errno) => {
            println!("errno: {}", errno);
            if errno == nc::EBADF {
                println!("Error: bad file descriptor");
            }
        }
    }

    let stdout = 1;
    let nwrite = unsafe { nc::write(stdout, msg.as_bytes()) };
    match nwrite {
        Ok(n) => {
            println!("nwrite: {}", n);
        }
        Err(errno) => {
            println!("errno: {}", errno);
        }
    }
}
