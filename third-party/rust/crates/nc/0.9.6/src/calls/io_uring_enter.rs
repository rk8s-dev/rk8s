/// Initiate and/or complete asynchronous I/O
pub unsafe fn io_uring_enter(
    fd: i32,
    to_submit: u32,
    min_complete: u32,
    flags: u32,
    arg: *const core::ffi::c_void,
    arg_size: usize,
) -> Result<i32, Errno> {
    let fd = fd as usize;
    let to_submit = to_submit as usize;
    let min_complete = min_complete as usize;
    let flags = flags as usize;
    let arg = arg as usize;
    syscall6(
        SYS_IO_URING_ENTER,
        fd,
        to_submit,
        min_complete,
        flags,
        arg,
        arg_size,
    )
    .map(|ret| ret as i32)
}
