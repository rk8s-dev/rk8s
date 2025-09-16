/// Places the current process into capability mode.
pub unsafe fn cap_enter() -> Result<(), Errno> {
    syscall0(SYS_CAP_ENTER).map(drop)
}
