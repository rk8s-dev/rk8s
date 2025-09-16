/// Lookup or change the realtime or idle priority of a process,
/// or the calling thread
pub unsafe fn rtprio_thread(function: i32, lwpid: lwpid_t, rt: &mut rtprio_t) -> Result<(), Errno> {
    let function = function as usize;
    let lwpid = lwpid as usize;
    let rt_ptr = rt as *mut rtprio_t as usize;
    syscall3(SYS_RTPRIO_THREAD, function, lwpid, rt_ptr).map(drop)
}
