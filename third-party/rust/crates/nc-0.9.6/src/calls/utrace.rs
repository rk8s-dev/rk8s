/// Insert user record in ktrace log
pub unsafe fn utrace(addr: usize, len: size_t) -> Result<(), Errno> {
    syscall2(SYS_UTRACE, addr, len).map(drop)
}
