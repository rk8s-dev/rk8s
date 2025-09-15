/// create an anonymous RAM-based file to access secret memory regions.
pub unsafe fn memfd_secret(flags: u32) -> Result<i32, Errno> {
    let flags = flags as usize;
    syscall1(SYS_MEMFD_SECRET, flags).map(|ret| ret as i32)
}
