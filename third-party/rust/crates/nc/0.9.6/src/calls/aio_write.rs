/// Asynchronous write to a file (REALTIME)
pub unsafe fn aio_write(job: &mut aiocb_t) -> Result<(), Errno> {
    let job_ptr = job as *mut aiocb_t as usize;
    syscall1(SYS_AIO_WRITE, job_ptr).map(drop)
}
