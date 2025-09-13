/// Unlock a semaphore.
pub unsafe fn _ksem_post(id: intptr_t) -> Result<(), Errno> {
    let id = id as usize;
    syscall1(SYS__KSEM_POST, id).map(drop)
}
