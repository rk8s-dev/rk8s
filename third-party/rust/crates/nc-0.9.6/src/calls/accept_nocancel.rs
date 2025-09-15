/// Accept a connection on a socket.
pub unsafe fn accept_nocancel(
    sockfd: i32,
    addr: &mut sockaddr_in_t,
    addrlen: &mut socklen_t,
) -> Result<i32, Errno> {
    let sockfd = sockfd as usize;
    let addr_ptr = addr as *mut sockaddr_in_t as usize;
    let addrlen_ptr = addrlen as *mut socklen_t as usize;
    syscall3(SYS_ACCEPT_NOCANCEL, sockfd, addr_ptr, addrlen_ptr).map(|val| val as i32)
}
