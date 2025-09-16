/// Execute a new program relative to a directory file descriptor.
///
/// # Examples
///
/// Specify program file via `filename`:
///
/// ```
/// let args = ["ls", "-l", "-a"];
/// let env = ["DISPLAY=:0"];
/// let ret = unsafe { nc::execveat(nc::AT_FDCWD, "/bin/ls", &args, &env, 0) };
/// assert!(ret.is_ok());
/// ```
///
/// Or via an opened file descriptor `fd`, leaving `filename` empty:
///
/// ```
/// let args = ["ls", "-l", "-a"];
/// let env = ["DISPLAY=:0"];
/// let ret = unsafe { nc::openat(nc::AT_FDCWD, "/bin/ls", nc::O_RDONLY | nc::O_CLOEXEC, 0) };
/// assert!(ret.is_ok());
/// let fd = ret.unwrap();
/// let ret = unsafe { nc::execveat(fd, "", &args, &env, nc::AT_EMPTY_PATH) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn execveat<P: AsRef<Path>>(
    fd: i32,
    filename: P,
    argv: &[P],
    env: &[P],
    flags: i32,
) -> Result<(), Errno> {
    let fd = fd as usize;
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;

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

    let flags = flags as usize;
    syscall5(SYS_EXECVEAT, fd, filename_ptr, argv_ptr, env_ptr, flags).map(drop)
}
