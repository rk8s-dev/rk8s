/// Wait for the next completion of an aio request
pub unsafe fn aio_waitcomplete(
    job: &mut aiocb_t,
    timeout: Option<&timespec_t>,
) -> Result<ssize_t, Errno> {
    let job_ptr = job as *mut aiocb_t as usize;
    let timeout_ptr =
        core::mem::transmute::<Option<_>, &timespec_t>(timeout) as *const timespec_t as usize;
    syscall2(SYS_AIO_WAITCOMPLETE, job_ptr, timeout_ptr).map(|val| val as ssize_t)
}
