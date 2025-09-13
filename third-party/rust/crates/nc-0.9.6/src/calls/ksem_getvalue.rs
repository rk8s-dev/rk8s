/// Get the value of a semaphore.
pub unsafe fn ksem_getvalue(id: semid_t, value: &mut i32) -> Result<(), Errno> {
    let id = id as usize;
    let value_ptr = value as *mut i32 as usize;
    syscall2(SYS_KSEM_GETVALUE, id, value_ptr).map(drop)
}
