/// Asynchronous read from a file (REALTIME)
pub unsafe fn aio_readv(job: &mut aiocb_t) -> Result<(), Errno> {
    let job_ptr = job as *mut aiocb_t as usize;
    syscall1(SYS_AIO_READV, job_ptr).map(drop)
}
