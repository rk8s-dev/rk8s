// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

use std::env;

fn main() {
    const BUF_SIZE: usize = 1024;

    let path = env::args().nth(1).unwrap_or_else(|| ".".to_owned());
    let ret = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_DIRECTORY, 0) };
    assert!(ret.is_ok());
    let fd = ret.unwrap();
    let mut buf = [0; BUF_SIZE];

    loop {
        let ret = unsafe { nc::getdents64(fd, &mut buf) };
        assert!(ret.is_ok());
        let nread = ret.unwrap() as usize;
        if nread == 0 {
            break;
        }

        let buf_ptr: *const u8 = buf.as_ptr();
        let mut bpos: usize = 0;

        println!("--------------- nread={nread} ---------------");
        println!("inode#    file type  d_reclen  d_off   d_name");
        while bpos < nread {
            let d = buf_ptr.wrapping_add(bpos) as *mut nc::linux_dirent64_t;
            let d_ref: &nc::linux_dirent64_t = unsafe { &(*d) };
            let d_type = match d_ref.d_type {
                nc::DT_REG => "regular",
                nc::DT_DIR => "directory",
                nc::DT_FIFO => "FIFO",
                nc::DT_SOCK => "socket",
                nc::DT_LNK => "symlink",
                nc::DT_BLK => "block-dev",
                nc::DT_CHR => "char-dev",
                nc::DT_UNKNOWN => "dt unknown",
                _ => "other unknown",
            };

            if let Ok(name) = std::str::from_utf8(d_ref.name()) {
                println!(
                    "{: >8}  {:<10} {: >4} {: >12}  {}",
                    d_ref.d_ino, d_type, d_ref.d_reclen, d_ref.d_off as u32, name
                );
            } else {
                eprintln!("Invalid name: {:?}", d_ref.name());
            }

            bpos += d_ref.d_reclen as usize;
        }
    }

    //    println!(
    //        "offset of d_name is: {}",
    //        offset_of!(nc::linux_dirent_t, d_name)
    //    );
    //    println!(
    //        "offset of d_reclen is: {}",
    //        offset_of!(nc::linux_dirent_t, d_reclen)
    //    );

    let ret = unsafe { nc::close(fd) };
    assert!(ret.is_ok());
}
