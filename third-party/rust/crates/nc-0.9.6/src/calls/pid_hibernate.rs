/// Freeze the specified process (provided in args->pid), or find and freeze a PID.
///
/// When a process is specified, this call is blocking, otherwise we wake up the
/// freezer thread and do not block on a process being frozen.
pub unsafe fn pid_hibernate(pid: pid_t) -> Result<(), Errno> {
    let pid = pid as usize;
    syscall1(SYS_PID_HIBERNATE, pid).map(drop)
}
