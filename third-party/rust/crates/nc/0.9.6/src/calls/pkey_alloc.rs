/// Create a new protection key.
pub unsafe fn pkey_alloc(flags: usize, init_val: u32) -> Result<i32, Errno> {
    let init_val = init_val as usize;
    syscall2(SYS_PKEY_ALLOC, flags, init_val).map(|ret| ret as i32)
}
