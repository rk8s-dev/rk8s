/// Send message to a message queue.
///
/// # Examples
///
/// ```
/// let name = "nc-mq-timedsend";
/// let ret = unsafe {
///     nc::mq_open(
///         name,
///         nc::O_CREAT | nc::O_RDWR,
///         (nc::S_IRUSR | nc::S_IWUSR) as nc::umode_t,
///         None,
///     )
/// };
/// assert!(ret.is_ok());
/// let mq_id = ret.unwrap();
///
/// let mut attr = nc::mq_attr_t::default();
/// let ret = unsafe { nc::mq_getsetattr(mq_id, None, Some(&mut attr)) };
/// assert!(ret.is_ok());
///
/// let msg = "Hello, Rust";
/// let prio = 0;
/// let timeout = nc::timespec_t {
///     tv_sec: 1,
///     tv_nsec: 0,
/// };
/// let ret = unsafe { nc::mq_timedsend(mq_id, msg.as_bytes(), prio, &timeout) };
/// assert!(ret.is_ok());
///
/// let ret = unsafe { nc::mq_getsetattr(mq_id, None, Some(&mut attr)) };
/// assert!(ret.is_ok());
/// assert_eq!(attr.mq_curmsgs, 1);
///
/// let ret = unsafe { nc::close(mq_id) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::mq_unlink(name) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn mq_timedsend(
    mqdes: mqd_t,
    msg: &[u8],
    msg_prio: u32,
    abs_timeout: &timespec_t,
) -> Result<(), Errno> {
    let mqdes = mqdes as usize;
    let msg = CString::new(msg);
    let msg_ptr = msg.as_ptr() as usize;
    let msg_len = msg.len();
    let msg_prio = msg_prio as usize;
    let abs_timeout_ptr = abs_timeout as *const timespec_t as usize;
    syscall5(
        SYS_MQ_TIMEDSEND,
        mqdes,
        msg_ptr,
        msg_len,
        msg_prio,
        abs_timeout_ptr,
    )
    .map(drop)
}
