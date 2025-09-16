/// Decrement (lock) a semaphore.
pub unsafe fn ksem_timedwait(id: semid_t, abstime: &timespec_t) -> Result<(), Errno> {
    let id = id as usize;
    let abstime_ptr = abstime as *const timespec_t as usize;
    syscall2(SYS_KSEM_TIMEDWAIT, id, abstime_ptr).map(drop)
}
