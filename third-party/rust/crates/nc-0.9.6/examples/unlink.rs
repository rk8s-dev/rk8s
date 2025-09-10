// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

fn main() {
    #[cfg(feature = "std")]
    let path = std::path::Path::new("/tmp/nc-unlink");
    #[cfg(not(feature = "std"))]
    let path = "/tmp/nc.unlink";

    let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_WRONLY | nc::O_CREAT, 0o644) };
    assert!(ret.is_ok());
    let fd = ret.unwrap();
    let ret = unsafe { nc::close(fd) };
    assert!(ret.is_ok());
    let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, path, 0) };
    assert!(ret.is_ok());
}
