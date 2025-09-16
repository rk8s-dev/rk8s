/// Set a thread's CPU affinity mask.
pub unsafe fn _sched_setaffinity(
    pid: pid_t,
    lid: lwpid_t,
    size: size_t,
    cpuset: &cpuset_t,
) -> Result<(), Errno> {
    let pid = pid as usize;
    let lid = lid as usize;
    let cpuset_ptr = cpuset as *const cpuset_t as usize;
    syscall4(SYS__SCHED_SETAFFINITY, pid, lid, size, cpuset_ptr).map(drop)
}
