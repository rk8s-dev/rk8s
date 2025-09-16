/// Issue memory barriers on a set of threads.
///
/// @cmd:   Takes command values defined in enum `membarrier_cmd`.
/// @flags: Currently needs to be 0. For future extensions.
///
/// If this system call is not implemented, `-ENOSYS` is returned. If the
/// command specified does not exist, not available on the running
/// kernel, or if the command argument is invalid, this system call
/// returns `-EINVAL`. For a given command, with flags argument set to 0,
/// this system call is guaranteed to always return the same value until
/// reboot.
///
/// All memory accesses performed in program order from each targeted thread
/// is guaranteed to be ordered with respect to `sys_membarrier()`. If we use
/// the semantic `barrier()` to represent a compiler barrier forcing memory
/// accesses to be performed in program order across the barrier, and
/// `smp_mb()` to represent explicit memory barriers forcing full memory
/// ordering across the barrier, we have the following ordering table for
/// each pair of `barrier()`, `sys_membarrier()` and `smp_mb()`:
///
/// The pair ordering is detailed as (O: ordered, X: not ordered):
///
/// ```text
///                        barrier()   smp_mb() sys_membarrier()
///        barrier()          X           X            O
///        smp_mb()           X           O            O
///        sys_membarrier()   O           O            O
/// ```
pub unsafe fn membarrier(cmd: i32, flags: u32, cpuid: i32) -> Result<i32, Errno> {
    let cmd = cmd as usize;
    let flags = flags as usize;
    let cpuid = cpuid as usize;
    syscall3(SYS_MEMBARRIER, cmd, flags, cpuid).map(|ret| ret as i32)
}
