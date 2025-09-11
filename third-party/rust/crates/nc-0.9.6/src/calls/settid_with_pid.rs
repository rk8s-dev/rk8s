/// Set the per-thread override identity.
pub unsafe fn settid_with_pid(pid: pid_t, assume: i32) -> Result<(), Errno> {
    let pid = pid as usize;
    let assume = assume as usize;
    syscall2(SYS_SETTID_WITH_PID, pid, assume).map(drop)
}
