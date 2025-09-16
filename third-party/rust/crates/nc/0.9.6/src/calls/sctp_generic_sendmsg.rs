/// Send a message on a socket.
pub unsafe fn sctp_generic_sendmsg(
    sockfd: i32,
    msg: &[u8],
    to: &[&sockaddr_t],
    sinfo: &mut sctp_sndrcvinfo_t,
    flags: i32,
) -> Result<ssize_t, Errno> {
    let sockfd = sockfd as usize;
    let msg_ptr = msg.as_ptr() as usize;
    let msg_len = msg.len();
    let to_ptr = to.as_ptr() as usize;
    let to_len = to.len();
    let sinfo_ptr = sinfo as *mut sctp_sndrcvinfo_t as usize;
    let _flags = flags as usize;
    // FIXME(Shaohua): Parameter error
    syscall6(
        SYS_SCTP_GENERIC_SENDMSG,
        sockfd,
        msg_ptr,
        msg_len,
        to_ptr,
        to_len,
        sinfo_ptr,
        //flags,
    )
    .map(|ret| ret as ssize_t)
}
