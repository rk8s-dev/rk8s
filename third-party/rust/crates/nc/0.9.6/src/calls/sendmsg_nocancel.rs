/// Send a message on a socket. Allow sending ancillary data.
pub unsafe fn sendmsg_nocancel(sockfd: i32, msg: caddr_t, flags: i32) -> Result<ssize_t, Errno> {
    let sockfd = sockfd as usize;
    let msg_ptr = msg as usize;
    let flags = flags as usize;
    syscall3(SYS_SENDMSG_NOCANCEL, sockfd, msg_ptr, flags).map(|ret| ret as ssize_t)
}
