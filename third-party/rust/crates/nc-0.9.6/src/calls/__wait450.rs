/// Wait for process to change state.
pub unsafe fn __wait450(
    pid: pid_t,
    wstatus: &mut i32,
    options: i32,
    rusage: &mut rusage_t,
) -> Result<pid_t, Errno> {
    let pid = pid as usize;
    let wstatus_ptr = wstatus as *mut i32 as usize;
    let options = options as usize;
    let rusage_ptr = rusage as *mut rusage_t as usize;
    syscall4(SYS___WAIT450, pid, wstatus_ptr, options, rusage_ptr).map(|ret| ret as pid_t)
}
