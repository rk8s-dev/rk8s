/// Unload kld files.
pub unsafe fn kldunloadf(file_id: i32, flags: i32) -> Result<(), Errno> {
    let file_id = file_id as usize;
    let flags = flags as usize;
    syscall2(SYS_KLDUNLOAD, file_id, flags).map(drop)
}
