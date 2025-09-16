/// Free a protection key.
pub unsafe fn pkey_free(pkey: i32) -> Result<(), Errno> {
    let pkey = pkey as usize;
    syscall1(SYS_PKEY_FREE, pkey).map(drop)
}
