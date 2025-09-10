/// Setup a context for performing asynchronous I/O.
///
/// Sets up a submission queue (SQ) and completion queue (CQ) with at least `entries`,
/// and returns a file descriptor which can be used to perform subsequent operations
/// on the `io_uring` instance.
/// The submission and completion queues are shared between userspace and the kernel,
/// which eliminates the need to copy data when initiating and completing I/O.
pub unsafe fn io_uring_setup(entries: u32, params: &mut io_uring_params_t) -> Result<u32, Errno> {
    let entries = entries as usize;
    let params_ptr = params as *mut io_uring_params_t as usize;
    syscall2(SYS_IO_URING_SETUP, entries, params_ptr).map(|ret| ret as u32)
}
