// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

fn main() -> Result<(), nc::Errno> {
    let socket_fd = unsafe { nc::socket(nc::AF_INET, nc::SOCK_STREAM, 0)? };

    // For Linux, value is the queue length of pending packets.
    // See https://github.com/rust-lang/socket2/issues/49
    #[cfg(any(target_os = "linux", target_os = "android"))]
    let queue_len: i32 = 5;
    // For the others, just a boolean value for enable and disable.
    #[cfg(target_os = "freebsd")]
    let queue_len: i32 = 1;

    let ret = unsafe {
        nc::setsockopt(
            socket_fd,
            nc::IPPROTO_TCP,
            nc::TCP_FASTOPEN,
            &queue_len as *const i32 as *const _,
            std::mem::size_of_val(&queue_len) as nc::socklen_t,
        )
    };
    assert!(ret.is_ok());

    unsafe { nc::close(socket_fd) }
}
