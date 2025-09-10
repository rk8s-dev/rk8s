// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

fn main() {
    let target_dir = "/tmp/nc-mount";
    let ret = unsafe { nc::mkdirat(nc::AT_FDCWD, target_dir, 0o755) };
    assert!(ret.is_ok());

    let src_dir = "/etc";
    let fs_type = "";
    let mount_flags = nc::MS_BIND | nc::MS_RDONLY;
    let data = std::ptr::null_mut();
    let ret = unsafe { nc::mount(src_dir, target_dir, fs_type, mount_flags, data) };
    assert!(ret.is_ok());
    let ret = unsafe { nc::umount2(target_dir, nc::UMOUNT_NOFOLLOW) };
    assert!(ret.is_ok());
    let ret = unsafe { nc::unlinkat(nc::AT_FDCWD, target_dir, nc::AT_REMOVEDIR) };
    assert!(ret.is_ok());
}
