/// Destroy an unamed semaphore.
pub unsafe fn ksem_destroy(id: semid_t) -> Result<(), Errno> {
    let id = id as usize;
    syscall1(SYS_KSEM_DESTROY, id).map(drop)
}
