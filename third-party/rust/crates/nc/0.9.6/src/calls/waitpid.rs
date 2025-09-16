/// Wait for process to change state.
///
/// # Examples
///
/// ```
/// let pid = unsafe { nc::fork() };
///
/// match pid {
///     Err(errno) => {
///         eprintln!("fork() error: {}", nc::strerror(errno));
///         unsafe { nc::exit(1) };
///     }
///     Ok(0) => println!("[child] pid is: {}", unsafe { nc::getpid() }),
///     Ok(pid) => {
///         let mut status = 0;
///         let ret = unsafe { nc::waitpid(pid, &mut status, 0) };
///         assert!(ret.is_ok());
///         let exited_pid = ret.unwrap();
///         assert_eq!(exited_pid, pid);
///     }
/// }
/// ```
pub unsafe fn waitpid(pid: pid_t, status: &mut i32, options: i32) -> Result<pid_t, Errno> {
    let pid = pid as usize;
    let status_ptr = status as *mut i32 as usize;
    let options = options as usize;
    syscall3(SYS_WAITPID, pid, status_ptr, options).map(|ret| ret as pid_t)
}
