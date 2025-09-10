/// Send a message on a socket.
pub unsafe fn sendto_nocancel(
    sockfd: i32,
    buf: &[u8],
    flags: i32,
    dest_addr: caddr_t,
    addrlen: socklen_t,
) -> Result<ssize_t, Errno> {
    let sockfd = sockfd as usize;
    let buf_ptr = buf.as_ptr() as usize;
    let len = buf.len();
    let flags = flags as usize;
    let dest_addr_ptr = dest_addr as *const sockaddr_in_t as usize;
    let addrlen = addrlen as usize;
    syscall6(
        SYS_SENDTO_NOCANCEL,
        sockfd,
        buf_ptr,
        len,
        flags,
        dest_addr_ptr,
        addrlen,
    )
    .map(|ret| ret as ssize_t)
}
