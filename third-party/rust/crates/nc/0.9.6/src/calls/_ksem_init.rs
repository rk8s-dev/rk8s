/// Initialize an unnamed semaphore.
pub unsafe fn _ksem_init(value: u32, id: &mut intptr_t) -> Result<(), Errno> {
    let value = value as usize;
    let id_ptr = id as *mut intptr_t as usize;
    syscall2(SYS__KSEM_INIT, value, id_ptr).map(drop)
}
