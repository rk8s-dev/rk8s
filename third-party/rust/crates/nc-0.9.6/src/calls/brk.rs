/// Change data segment size.
pub unsafe fn brk(addr: usize) -> Result<(), Errno> {
    syscall1(SYS_BRK, addr).map(drop)
}
