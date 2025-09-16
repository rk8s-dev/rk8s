/// Asynchronous mlock operation
pub unsafe fn aio_mlock(job: &mut aiocb_t) -> Result<(), Errno> {
    let job_ptr = job as *mut aiocb_t as usize;
    syscall1(SYS_AIO_MLOCK, job_ptr).map(drop)
}
