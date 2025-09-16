/// Obtain handle for an open file
pub unsafe fn open_by_handle_at(
    mount_fd: i32,
    handle: &mut file_handle_t,
    flags: i32,
) -> Result<i32, Errno> {
    let mount_fd = mount_fd as usize;
    let handle_ptr = handle as *mut file_handle_t as usize;
    let flags = flags as usize;
    syscall3(SYS_OPEN_BY_HANDLE_AT, mount_fd, handle_ptr, flags).map(|ret| ret as i32)
}
