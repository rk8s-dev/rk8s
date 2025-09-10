/// Retrieve NUMA memory policy for a thread
pub unsafe fn get_mempolicy(
    mode: &mut i32,
    nmask: &mut usize,
    max_node: usize,
    addr: *const core::ffi::c_void,
    flags: usize,
) -> Result<(), Errno> {
    let mode_ptr = mode as *mut i32 as usize;
    let nmask_ptr = nmask as *mut usize as usize;
    let addr = addr as usize;
    syscall5(
        SYS_GET_MEMPOLICY,
        mode_ptr,
        nmask_ptr,
        max_node,
        addr,
        flags,
    )
    .map(drop)
}
