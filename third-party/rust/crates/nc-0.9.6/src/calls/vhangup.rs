/// Virtually hang up the current terminal.
pub unsafe fn vhangup() -> Result<(), Errno> {
    syscall0(SYS_VHANGUP).map(drop)
}
