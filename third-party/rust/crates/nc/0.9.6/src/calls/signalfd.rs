/// Create a file descriptor to accept signals.
pub unsafe fn signalfd(fd: i32, mask: &sigset_t) -> Result<i32, Errno> {
    let fd = fd as usize;
    let mask_ptr = mask as *const sigset_t as usize;
    let size_mask = core::mem::size_of::<sigset_t>();
    syscall3(SYS_SIGNALFD, fd, mask_ptr, size_mask).map(|ret| ret as i32)
}
