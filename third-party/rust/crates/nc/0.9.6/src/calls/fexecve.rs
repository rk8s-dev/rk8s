/// Execute a new program.
///
/// # Examples
///
/// ```
/// let args = ["ls", "-l", "-a"];
/// let env = ["DISPLAY=:0"];
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, "/bin/ls", nc::O_RDONLY, 0) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::fexecve(fd, &args, &env) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn fexecve<P: AsRef<Path>>(fd: i32, argv: &[P], env: &[P]) -> Result<(), Errno> {
    let fd = fd as usize;
    // Construct argument list.
    let argv_data: Vec<CString> = argv.iter().map(|arg| CString::new(arg.as_ref())).collect();
    let mut argv_data_ptr: Vec<*const u8> = argv_data.iter().map(|arg| arg.as_ptr()).collect();
    // Null-terminated
    argv_data_ptr.push(core::ptr::null::<u8>());
    let argv_ptr = argv_data_ptr.as_ptr() as usize;

    // Construct environment list.
    let env_data: Vec<CString> = env.iter().map(|item| CString::new(item.as_ref())).collect();
    let mut env_data_ptr: Vec<*const u8> = env_data.iter().map(|item| item.as_ptr()).collect();
    // Null-terminated
    env_data_ptr.push(core::ptr::null::<u8>());
    let env_ptr = env_data_ptr.as_ptr() as usize;

    syscall3(SYS_FEXECVE, fd, argv_ptr, env_ptr).map(drop)
}
