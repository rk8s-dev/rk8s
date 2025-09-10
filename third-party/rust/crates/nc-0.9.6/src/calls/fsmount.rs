/// Create a kernel mount representation for a new, prepared superblock.
pub unsafe fn fsmount(fs_fd: i32, flags: u32, attr_flags: u32) -> Result<i32, Errno> {
    let fs_fd = fs_fd as usize;
    let flags = flags as usize;
    let attr_flags = attr_flags as usize;
    syscall3(SYS_FSMOUNT, fs_fd, flags, attr_flags).map(|ret| ret as i32)
}
