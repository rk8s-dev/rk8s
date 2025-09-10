/// Used by the NFS daemons to pass information into and out of the kernel and
/// also to enter the kernel as a server daemon.
pub unsafe fn nfssvc(fd: i32, args: usize) -> Result<(), Errno> {
    let fd = fd as usize;
    syscall2(SYS_NFSSVC, fd, args).map(drop)
}
