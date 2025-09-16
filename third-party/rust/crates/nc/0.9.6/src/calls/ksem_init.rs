/// Initialize an unamed semaphore.
pub unsafe fn ksem_init(value: u32, id: &mut semid_t) -> Result<(), Errno> {
    let value = value as usize;
    let id_ptr = id as *mut semid_t as usize;
    syscall2(SYS_KSEM_INIT, value, id_ptr).map(drop)
}
