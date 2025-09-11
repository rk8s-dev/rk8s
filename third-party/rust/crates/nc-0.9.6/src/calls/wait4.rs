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
///         let ret = unsafe { nc::wait4(-1, Some(&mut status), 0, None) };
///         assert!(ret.is_ok());
///         println!("status: {}", status);
///         let exited_pid = ret.unwrap();
///         assert_eq!(exited_pid, pid);
///     }
/// }
/// ```
pub unsafe fn wait4(
    pid: pid_t,
    wstatus: Option<&mut i32>,
    options: i32,
    rusage: Option<&mut rusage_t>,
) -> Result<pid_t, Errno> {
    let pid = pid as usize;
    let wstatus_ptr = wstatus.map_or(core::ptr::null_mut::<i32>() as usize, |wstatus| {
        wstatus as *mut i32 as usize
    });
    let options = options as usize;
    let rusage_ptr = rusage.map_or(core::ptr::null_mut::<rusage_t>() as usize, |rusage| {
        rusage as *mut rusage_t as usize
    });
    syscall4(SYS_WAIT4, pid, wstatus_ptr, options, rusage_ptr).map(|ret| ret as pid_t)
}
