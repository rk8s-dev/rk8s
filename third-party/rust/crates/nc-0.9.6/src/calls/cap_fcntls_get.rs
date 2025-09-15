/// Get the list of allowed `fcntl()` commands if a file descriptor
/// is granted the CAP_FCNTL capability right,
pub unsafe fn cap_fcntls_get(fd: i32, fcntl_rights: &mut u32) -> Result<(), Errno> {
    let fd = fd as usize;
    let fcntl_rights_ptr = fcntl_rights as *mut u32 as usize;
    syscall2(SYS_CAP_FCNTLS_GET, fd, fcntl_rights_ptr).map(drop)
}
