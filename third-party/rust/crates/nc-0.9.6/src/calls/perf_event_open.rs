/// Set up performance monitoring.
pub unsafe fn perf_event_open(
    attr: &mut perf_event_attr_t,
    pid: pid_t,
    cpu: i32,
    group_fd: i32,
    flags: usize,
) -> Result<i32, Errno> {
    let attr_ptr = attr as *mut perf_event_attr_t as usize;
    let pid = pid as usize;
    let cpu = cpu as usize;
    let group_fd = group_fd as usize;
    syscall5(SYS_PERF_EVENT_OPEN, attr_ptr, pid, cpu, group_fd, flags).map(|ret| ret as i32)
}
