/// Open a message queue.
///
/// # Examples
///
/// ```
/// let name = "nc-posix-mq";
/// let ret = unsafe {
///     nc::mq_open(
///         name,
///         nc::O_CREAT | nc::O_RDWR,
///         (nc::S_IRUSR | nc::S_IWUSR) as nc::mode_t,
///         None,
///     )
/// };
/// assert!(ret.is_ok());
/// let mq_id = ret.unwrap();
/// let ret = unsafe { nc::close(mq_id) };
/// assert!(ret.is_ok());
/// let ret = unsafe { nc::mq_unlink(name) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn mq_open<P: AsRef<Path>>(
    name: P,
    oflag: i32,
    mode: mode_t,
    attr: Option<&mut mq_attr_t>,
) -> Result<mqd_t, Errno> {
    let name = CString::new(name.as_ref());
    let name_ptr = name.as_ptr() as usize;
    let oflag = oflag as usize;
    let mode = mode as usize;
    let attr_ptr = attr.map_or(0, |attr| attr as *mut mq_attr_t as usize);
    syscall4(SYS_MQ_OPEN, name_ptr, oflag, mode, attr_ptr).map(|ret| ret as mqd_t)
}
