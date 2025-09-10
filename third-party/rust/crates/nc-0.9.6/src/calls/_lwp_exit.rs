/// Terminate thread.
pub unsafe fn _lwp_exit() -> Result<i32, Errno> {
    syscall0(SYS__LWP_EXIT).map(|val| val as i32)
}
