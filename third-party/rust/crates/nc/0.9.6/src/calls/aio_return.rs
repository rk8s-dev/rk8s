/// Retrieve return status of asynchronous I/O operation (REALTIME)
pub unsafe fn aio_return(job: &mut aiocb_t) -> Result<ssize_t, Errno> {
    let job_ptr = job as *mut aiocb_t as usize;
    syscall1(SYS_AIO_RETURN, job_ptr).map(|val| val as ssize_t)
}
