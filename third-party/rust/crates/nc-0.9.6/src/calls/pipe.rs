/// Create a pipe.
///
/// # Examples
///
/// ```
/// let mut fds = [-1_i32, 2];
/// let ret = unsafe { nc::pipe(&mut fds) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fds[0]) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fds[1]) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn pipe(pipefd: &mut [i32; 2]) -> Result<(), Errno> {
    let pipefd_ptr = pipefd.as_mut_ptr() as usize;
    syscall1(SYS_PIPE, pipefd_ptr).map(drop)
}
