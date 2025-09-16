/// Attempts to cancel an iocb previously passed to `io_submit`.
///
/// If the operation is successfully cancelled, the resulting event is
/// copied into the memory pointed to by `result` without being placed
/// into the completion queue and 0 is returned.
///
///
/// # Errors
/// - May fail with `-EFAULT` if any of the data structures pointed to are invalid.
/// - May fail with `-EINVAL` if `aio_context` specified by `ctx_id` is invalid.
/// - May fail with `-EAGAIN` if the iocb specified was not cancelled.
/// - Will fail with `-ENOSYS` if not implemented.
pub unsafe fn io_cancel(
    ctx_id: aio_context_t,
    iocb: &iocb_t,
    result: &mut io_event_t,
) -> Result<(), Errno> {
    let iocb_ptr = iocb as *const iocb_t as usize;
    let result_ptr = result as *mut io_event_t as usize;
    syscall3(SYS_IO_CANCEL, ctx_id, iocb_ptr, result_ptr).map(drop)
}
