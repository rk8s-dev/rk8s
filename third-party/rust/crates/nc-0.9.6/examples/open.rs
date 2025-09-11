// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

fn main() -> Result<(), nc::Errno> {
    let path = "/tmp/hello.rs";

    #[cfg(target_os = "freebsd")]
    let fd = unsafe {
        nc::open(
            path,
            nc::O_CREAT | nc::O_RDWR,
            nc::S_IRUSR | nc::S_IWUSR | nc::S_IRGRP | nc::S_IROTH,
        )?
    };

    #[cfg(any(target_os = "linux", target_os = "android"))]
    let fd = unsafe {
        nc::openat(
            nc::AT_FDCWD,
            path,
            nc::O_CREAT | nc::O_RDWR,
            nc::S_IRUSR | nc::S_IWUSR | nc::S_IRGRP | nc::S_IROTH,
        )?
    };

    let msg = b"fn main() { println!(\"Hello, world\");}";

    let n_write = unsafe { nc::write(fd, msg) };
    assert!(n_write.is_ok());
    let ret = unsafe { nc::close(fd) };
    assert!(ret.is_ok());
    Ok(())
}
