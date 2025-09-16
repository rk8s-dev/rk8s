pub unsafe fn set_mempolicy_home_node(
    start: *const core::ffi::c_void,
    len: usize,
    home_node: usize,
    flags: usize,
) -> Result<(), Errno> {
    let start = start as usize;
    syscall4(SYS_SET_MEMPOLICY_HOME_NODE, start, len, home_node, flags).map(drop)
}
