/// Set the robust-futex list head of a task.
///
/// Params:
/// - `head`: pointer to the list-head
/// - `len`: length of the list-head, as userspace expects
pub unsafe fn set_robust_list(head: *mut robust_list_head_t, len: usize) -> Result<(), Errno> {
    let head_ptr = head as usize;
    syscall2(SYS_SET_ROBUST_LIST, head_ptr, len).map(drop)
}
