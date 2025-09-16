/// Receive a message from a message queue
///
/// # Examples
///
/// ```
/// let name = "nc-mq-timedreceive";
/// let ret = unsafe {
///     nc::mq_open(
///         name,
///         nc::O_CREAT | nc::O_RDWR | nc::O_EXCL,
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
/// println!("attr: {:?}", attr);
///
/// let msg = "Hello, Rust";
/// let prio = 42;
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
/// let mut buf = vec![0_u8; attr.mq_msgsize as usize];
/// let mut recv_prio = 0;
/// let read_timeout = nc::timespec_t {
///     tv_sec: 1,
///     tv_nsec: 0,
/// };
/// let ret = unsafe { nc::mq_timedreceive(mq_id, &mut buf, &mut recv_prio, &read_timeout) };
/// if let Err(errno) = ret {
///     eprintln!("mq_timedreceive() error: {}", nc::strerror(errno));
/// }
/// assert!(ret.is_ok());
/// let n_read = ret.unwrap() as usize;
/// assert_eq!(n_read, msg.len());
///
/// let ret = unsafe { nc::close(mq_id) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::mq_unlink(name) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn mq_timedreceive(
    mqdes: mqd_t,
    msg: &mut [u8],
    msg_prio: &mut u32,
    abs_timeout: &timespec_t,
) -> Result<ssize_t, Errno> {
    let mqdes = mqdes as usize;
    let msg_ptr = msg.as_mut_ptr() as usize;
    let msg_len = msg.len();
    let msg_prio = msg_prio as *mut u32 as usize;
    let abs_timeout_ptr = abs_timeout as *const timespec_t as usize;
    syscall5(
        SYS_MQ_TIMEDRECEIVE,
        mqdes,
        msg_ptr,
        msg_len,
        msg_prio,
        abs_timeout_ptr,
    )
    .map(|ret| ret as ssize_t)
}
