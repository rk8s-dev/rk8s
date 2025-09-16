/// Cancel an outstanding asynchronous I/O operation (REALTIME)
pub unsafe fn aio_cancel(fd: i32, job: &mut aiocb_t) -> Result<i32, Errno> {
    let fd = fd as usize;
    let job_ptr = job as *mut aiocb_t as usize;
    syscall2(SYS_AIO_CANCEL, fd, job_ptr).map(|val| val as i32)
}
