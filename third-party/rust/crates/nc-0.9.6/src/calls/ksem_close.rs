/// Close an semaphore.
pub unsafe fn ksem_close(id: semid_t) -> Result<(), Errno> {
    let id = id as usize;
    syscall1(SYS_KSEM_CLOSE, id).map(drop)
}
