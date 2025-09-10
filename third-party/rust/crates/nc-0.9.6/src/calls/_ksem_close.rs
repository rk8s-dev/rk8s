/// Close a semaphore.
pub unsafe fn _ksem_close(id: intptr_t) -> Result<(), Errno> {
    let id = id as usize;
    syscall1(SYS__KSEM_CLOSE, id).map(drop)
}
