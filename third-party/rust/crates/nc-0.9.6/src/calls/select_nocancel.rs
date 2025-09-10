/// Sychronous I/O multiplexing.
pub unsafe fn select_nocancel(
    nfds: i32,
    readfds: *mut u32,
    writefds: *mut u32,
    exceptfds: *mut u32,
    timeout: &mut timeval_t,
) -> Result<i32, Errno> {
    let nfds = nfds as usize;
    let readfds_ptr = readfds as usize;
    let writefds_ptr = writefds as usize;
    let exceptfds_ptr = exceptfds as usize;
    let timeout_ptr = timeout as *mut timeval_t as usize;
    syscall5(
        SYS_SELECT_NOCANCEL,
        nfds,
        readfds_ptr,
        writefds_ptr,
        exceptfds_ptr,
        timeout_ptr,
    )
    .map(|ret| ret as i32)
}
