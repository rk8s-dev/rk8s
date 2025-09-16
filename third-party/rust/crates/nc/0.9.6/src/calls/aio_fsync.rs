/// Asynchronous file synchronization (REALTIME)
pub unsafe fn aio_fsync(op: i32, job: &mut aiocb_t) -> Result<(), Errno> {
    let op = op as usize;
    let job_ptr = job as *mut aiocb_t as usize;
    syscall2(SYS_AIO_FSYNC, op, job_ptr).map(drop)
}
