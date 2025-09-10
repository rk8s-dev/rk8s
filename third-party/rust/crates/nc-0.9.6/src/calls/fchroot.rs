/// Change the root directory.
pub unsafe fn fchroot(fd: i32) -> Result<(), Errno> {
    let fd = fd as usize;
    syscall1(SYS_FCHROOT, fd).map(drop)
}
