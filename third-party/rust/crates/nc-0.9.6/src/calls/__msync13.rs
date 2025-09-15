/// Synchronize a file with memory map.
pub unsafe fn __msync13(addr: usize, len: size_t, flags: i32) -> Result<(), Errno> {
    let flags = flags as usize;
    syscall3(SYS___MSYNC13, addr, len, flags).map(drop)
}
