// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

fn main() -> Result<(), nc::Errno> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    let fd = unsafe { nc::openat(nc::AT_FDCWD, "/etc/passwd", nc::O_RDONLY, 0)? };

    #[cfg(target_os = "freebsd")]
    let fd = unsafe { nc::open("/etc/passwd", nc::O_RDONLY, 0)? };

    let mut buf: [u8; 256] = [0; 256];
    loop {
        let n_read = unsafe { nc::read(fd, &mut buf) };
        match n_read {
            Ok(n) => {
                if n == 0 {
                    break;
                }
                // FIXME(Shaohua): Read buf with len(n).
                if let Ok(s) = std::str::from_utf8(&buf) {
                    print!("s: {}", s);
                } else {
                    eprintln!("Failed to read buf as UTF-8!");
                    break;
                }
            }
            Err(errno) => {
                eprintln!("Failed to read, got errno: {}", errno);
                break;
            }
        }
    }

    unsafe { nc::close(fd) }
}
