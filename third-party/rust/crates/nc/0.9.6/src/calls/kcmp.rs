/// Compare two processes to determine if they share a kernel resource.
pub unsafe fn kcmp(
    pid1: pid_t,
    pid2: pid_t,
    type_: i32,
    idx1: usize,
    idx2: usize,
) -> Result<i32, Errno> {
    let pid1 = pid1 as usize;
    let pid2 = pid2 as usize;
    let type_ = type_ as usize;
    syscall5(SYS_KCMP, pid1, pid2, type_, idx1, idx2).map(|ret| ret as i32)
}
