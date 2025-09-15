/// Change ownership of a file.
pub unsafe fn __posix_fchown(fd: i32, user: uid_t, group: gid_t) -> Result<(), Errno> {
    let fd = fd as usize;
    let user = user as usize;
    let group = group as usize;
    syscall3(SYS___POSIX_FCHOWN, fd, user, group).map(drop)
}
