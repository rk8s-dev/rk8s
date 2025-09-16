/// Send a signal to specific process, based on process descriptor.
pub unsafe fn pdkill(fd: i32, sig: i32) -> Result<(), Errno> {
    let fd = fd as usize;
    let sig = sig as usize;
    syscall2(SYS_PDKILL, fd, sig).map(drop)
}
