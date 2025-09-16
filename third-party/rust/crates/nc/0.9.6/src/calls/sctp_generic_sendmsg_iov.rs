/// Send a message on a socket.
pub unsafe fn sctp_generic_sendmsg_iov(
    sockfd: i32,
    iov: &mut [iovec_t],
    to: &[&sockaddr_t],
    sinfo: &mut sctp_sndrcvinfo_t,
    flags: i32,
) -> Result<ssize_t, Errno> {
    let sockfd = sockfd as usize;
    let iov_ptr = iov.as_mut_ptr() as usize;
    let iov_len = iov.len();
    let to_ptr = to.as_ptr() as usize;
    let to_len = to.len();
    let sinfo_ptr = sinfo as *mut sctp_sndrcvinfo_t as usize;
    let _flags = flags as usize;
    // FIXME(Shaohua): Parameter error
    syscall6(
        SYS_SCTP_GENERIC_SENDMSG_IOV,
        sockfd,
        iov_ptr,
        iov_len,
        to_ptr,
        to_len,
        sinfo_ptr,
        //flags,
    )
    .map(|ret| ret as ssize_t)
}
