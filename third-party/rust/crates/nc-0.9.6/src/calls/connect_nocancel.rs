/// Initialize a connection on a socket.
pub unsafe fn connect_nocancel(
    sockfd: i32,
    name: caddr_t,
    addrlen: socklen_t,
) -> Result<(), Errno> {
    let sockfd = sockfd as usize;
    let name_ptr = name as usize;
    let addrlen = addrlen as usize;
    syscall3(SYS_CONNECT_NOCANCEL, sockfd, name_ptr, addrlen).map(drop)
}
