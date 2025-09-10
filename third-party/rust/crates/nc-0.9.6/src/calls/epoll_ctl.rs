/// Control interface for an epoll file descriptor.
///
/// # Examples
///
/// ```
/// let epfd = unsafe { nc::epoll_create1(nc::EPOLL_CLOEXEC) };
/// assert!(epfd.is_ok());
/// let epfd = epfd.unwrap();
/// let mut fds: [i32; 2] = [0, 0];
/// let ret = unsafe { nc::pipe2(&mut fds, 0) };
/// assert!(ret.is_ok());
/// let mut event = nc::epoll_event_t::default();
/// event.events = nc::EPOLLIN | nc::EPOLLET;
/// event.data.fd = fds[0];
/// let ctl_ret = unsafe { nc::epoll_ctl(epfd, nc::EPOLL_CTL_ADD, fds[0], &mut event) };
/// assert!(ctl_ret.is_ok());
/// let ret = unsafe { nc::close(fds[0]) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(fds[1]) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::close(epfd) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn epoll_ctl(
    epfd: i32,
    op: i32,
    fd: i32,
    event: &mut epoll_event_t,
) -> Result<(), Errno> {
    let epfd = epfd as usize;
    let op = op as usize;
    let fd = fd as usize;
    let event_ptr = event as *mut epoll_event_t as usize;
    syscall4(SYS_EPOLL_CTL, epfd, op, fd, event_ptr).map(drop)
}
