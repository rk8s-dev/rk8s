/// Lookup or change the realtime or idle priority of a process,
/// or the calling thread
pub unsafe fn rtprio(function: i32, pid: pid_t, rt: &mut rtprio_t) -> Result<(), Errno> {
    let function = function as usize;
    let pid = pid as usize;
    let rt_ptr = rt as *mut rtprio_t as usize;
    syscall3(SYS_RTPRIO, function, pid, rt_ptr).map(drop)
}
