/// Is current process tainted by uid or gid changes
///
/// Returns 1 if the process environment or memory address space is considered “tainted”,
/// and returns 0 otherwise.
#[must_use]
pub unsafe fn issetugid() -> i32 {
    // This function is always successful.
    syscall0(SYS_ISSETUGID)
        .map(|val| val as i32)
        .expect("issetugid() failed")
}
