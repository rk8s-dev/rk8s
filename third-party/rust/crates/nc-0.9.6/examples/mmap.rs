// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

use std::ffi::c_void;
use std::ptr;

fn main() {
    let path = "/etc/passwd";
    let fd = unsafe { nc::openat(nc::AT_FDCWD, path, nc::O_RDONLY, 0o644) };
    assert!(fd.is_ok());
    let fd = fd.unwrap();

    let mut sb = nc::stat_t::default();
    let ret = unsafe { nc::fstat(fd, &mut sb) };
    assert!(ret.is_ok());

    let offset: usize = 0;
    let length: usize = sb.st_size as usize - offset;
    // Offset for mmap must be page aligned.
    let pa_offset: usize = offset & !(nc::PAGE_SIZE - 1);
    let map_length = length + offset - pa_offset;

    #[cfg(target_arch = "arm")]
    let addr = unsafe {
        nc::mmap2(
            ptr::null(),
            map_length,
            nc::PROT_READ,
            nc::MAP_PRIVATE,
            fd,
            pa_offset as nc::off_t,
        )
    };
    #[cfg(not(target_arch = "arm"))]
    let addr = unsafe {
        nc::mmap(
            ptr::null(),
            map_length,
            nc::PROT_READ,
            nc::MAP_PRIVATE,
            fd,
            pa_offset as nc::off_t,
        )
    };
    assert!(addr.is_ok());
    let addr: *const c_void = addr.unwrap();

    let stdout = 1;
    // Create the "fat pointer".
    // let buf = unsafe {
    //     mem::transmute::<(usize, usize), &[u8]>((addr as usize + offset - pa_offset, length))
    // };
    // Create buffer slice.
    let buf: &[u8] = unsafe {
        &*ptr::slice_from_raw_parts(
            addr.wrapping_add(offset)
                .wrapping_sub(pa_offset)
                .cast::<u8>(),
            length,
        )
    };
    let n_write = unsafe { nc::write(stdout, buf) };
    assert!(n_write.is_ok());
    assert_eq!(n_write, Ok(length as nc::ssize_t));
    let ret = unsafe { nc::munmap(addr, map_length) };
    assert!(ret.is_ok());
    let ret = unsafe { nc::close(fd) };
    assert!(ret.is_ok());
}
