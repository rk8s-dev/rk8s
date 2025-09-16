/// Create a child process.
pub unsafe fn clone(
    clone_flags: usize,
    child_stack: *const core::ffi::c_void,
    parent_tid: Option<&mut pid_t>,
    tls: Option<*const core::ffi::c_void>,
    child_tid: Option<&mut pid_t>,
) -> Result<pid_t, Errno> {
    use core::ptr::null_mut;
    let child_stack = child_stack as usize;
    let parent_tid_ptr = parent_tid.map_or(null_mut::<pid_t>() as usize, |parent_tid| {
        parent_tid as *mut pid_t as usize
    });
    let child_tid_ptr = child_tid.map_or(null_mut::<pid_t>() as usize, |child_tid| {
        child_tid as *mut pid_t as usize
    });
    let tls_ptr = tls.map_or(core::ptr::null::<u8>() as usize, |tls| tls as usize);
    syscall5(
        SYS_CLONE,
        clone_flags,
        child_stack,
        parent_tid_ptr,
        tls_ptr,
        child_tid_ptr,
    )
    .map(|ret| ret as pid_t)
}
