pub unsafe fn r#break(addr: usize) -> Result<(), Errno> {
    syscall1(SYS_BREAK, addr).map(drop)
}
