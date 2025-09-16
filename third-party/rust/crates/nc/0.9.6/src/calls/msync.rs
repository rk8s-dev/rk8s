/// Synchronize a file with memory map.
pub unsafe fn msync(addr: *const core::ffi::c_void, len: size_t, flags: i32) -> Result<(), Errno> {
    let addr = addr as usize;
    let flags = flags as usize;
    syscall3(SYS_MSYNC, addr, len, flags).map(drop)
}
