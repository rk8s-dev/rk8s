/// Set thread-local storage information.
pub unsafe fn set_thread_area(addr: usize) -> Result<(), Errno> {
    syscall1(SYS_SET_THREAD_AREA, addr).map(drop)
}
