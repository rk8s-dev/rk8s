/// Get a thread's CPU affinity mask.
pub unsafe fn _sched_getaffinity(
    pid: pid_t,
    lid: lwpid_t,
    size: size_t,
    cpuset: &mut cpuset_t,
) -> Result<(), Errno> {
    let pid = pid as usize;
    let lid = lid as usize;
    let cpuset_ptr = cpuset as *mut cpuset_t as usize;
    syscall4(SYS__SCHED_GETAFFINITY, pid, lid, size, cpuset_ptr).map(drop)
}
