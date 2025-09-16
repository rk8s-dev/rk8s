/// Set default NUMA memory policy for a thread and its children
pub unsafe fn set_mempolicy(mode: i32, nmask: &[usize], max_node: usize) -> Result<(), Errno> {
    let mode = mode as usize;
    let nmask = nmask.as_ptr() as usize;
    syscall3(SYS_SET_MEMPOLICY, mode, nmask, max_node).map(drop)
}
