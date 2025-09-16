/// Suspend until asynchronous I/O operations or timeout complete (REALTIME)
pub unsafe fn aio_suspend(jobs: &[aiocb_t], timeout: &timespec_t) -> Result<(), Errno> {
    let jobs_ptr = jobs.as_ptr() as usize;
    let nent = jobs.len();
    let timeout_ptr = timeout as *const timespec_t as usize;
    syscall3(SYS_AIO_SUSPEND, jobs_ptr, nent, timeout_ptr).map(drop)
}
