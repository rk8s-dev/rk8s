// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

fn main() -> Result<(), nc::Errno> {
    let socket_fd = unsafe { nc::socket(nc::AF_INET, nc::SOCK_STREAM, 0)? };
    let interface_name = "lo";
    let ret = unsafe {
        nc::setsockopt(
            socket_fd,
            nc::SOL_SOCKET,
            nc::SO_BINDTODEVICE,
            interface_name.as_ptr() as *const _,
            interface_name.len() as nc::socklen_t,
        )
    };
    match ret {
        Err(errno) => eprintln!("socket() err: {}", nc::strerror(errno)),
        Ok(_) => println!("Now socket is bind to {}", interface_name),
    }
    let ret = unsafe { nc::close(socket_fd) };
    assert!(ret.is_ok());
    Ok(())
}
