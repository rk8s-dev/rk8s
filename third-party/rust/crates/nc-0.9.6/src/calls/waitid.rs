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
///         let mut info = nc::siginfo_t::default();
///         let options = nc::WEXITED;
///         let ret = unsafe { nc::waitid(nc::P_ALL, -1, &mut info, options, None) };
///         match ret {
///             Err(errno) => eprintln!("waitid() error: {}", nc::strerror(errno)),
///             Ok(()) => {
///                 let exited_pid = unsafe { info.siginfo.sifields.sigchld.pid };
///                 assert_eq!(pid, exited_pid);
///             }
///         }
///     }
/// }
/// ```
pub unsafe fn waitid(
    which: i32,
    pid: pid_t,
    info: &mut siginfo_t,
    options: i32,
    ru: Option<&mut rusage_t>,
) -> Result<(), Errno> {
    let which = which as usize;
    let pid = pid as usize;
    let info_ptr = info as *mut siginfo_t as usize;
    let options = options as usize;
    let ru_ptr = ru.map_or(core::ptr::null_mut::<rusage_t>() as usize, |ru| {
        ru as *mut rusage_t as usize
    });
    syscall5(SYS_WAITID, which, pid, info_ptr, options, ru_ptr).map(drop)
}
