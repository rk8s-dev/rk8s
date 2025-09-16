/// Destroy an unnamed semaphore
pub unsafe fn _ksem_destroy(id: intptr_t) -> Result<(), Errno> {
    let id = id as usize;
    syscall1(SYS__KSEM_DESTROY, id).map(drop)
}
