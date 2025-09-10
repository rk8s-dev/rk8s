/// Get the robust-futex list head of a task.
///
/// Params:
/// - `pid`: pid of the process `[zero for current task]`
/// - `head_ptr`: pointer to a list-head pointer, the kernel fills it in
/// - `len_ptr`: pointer to a length field, the kernel fills in the header size
pub unsafe fn get_robust_list(
    pid: pid_t,
    head_ptr: *mut *mut robust_list_head_t,
    len_ptr: &mut size_t,
) -> Result<(), Errno> {
    let pid = pid as usize;
    let head_ptr = head_ptr as usize;
    let len_ptr = len_ptr as *mut size_t as usize;
    syscall3(SYS_GET_ROBUST_LIST, pid, head_ptr, len_ptr).map(drop)
}
