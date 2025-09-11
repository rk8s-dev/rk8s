/// New api to create child process.
///
/// # Examples
///
/// ```
/// let mut args = nc::clone_args_t::default();
/// let mut pid_fd: i32 = -1;
/// args.exit_signal = nc::SIGCHLD as u64;
/// args.pidfd = &mut pid_fd as *mut i32 as usize as u64;
/// args.flags = nc::CLONE_PIDFD as u64 | nc::CLONE_PARENT_SETTID as u64;
/// let pid = unsafe { nc::clone3(&args) };
/// assert!(pid.is_ok());
/// ```
pub unsafe fn clone3(cl_args: &clone_args_t) -> Result<pid_t, Errno> {
    let cl_args_ptr = cl_args as *const clone_args_t as usize;
    let size = core::mem::size_of::<clone_args_t>();
    syscall2(SYS_CLONE3, cl_args_ptr, size).map(|ret| ret as pid_t)
}
