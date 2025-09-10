/// Set the RT priority of a thread.
pub unsafe fn sched_setattr(pid: pid_t, attr: &sched_attr_t, flags: u32) -> Result<(), Errno> {
    let pid = pid as usize;
    let attr_ptr = attr as *const sched_attr_t as usize;
    let flags = flags as usize;
    syscall3(SYS_SCHED_SETATTR, pid, attr_ptr, flags).map(drop)
}
