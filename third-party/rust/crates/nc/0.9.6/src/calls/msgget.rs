/// Get a System V message queue identifier.
///
/// # Examples
///
/// ```
/// let key = nc::IPC_PRIVATE;
/// let flags = nc::IPC_CREAT | nc::IPC_EXCL | (nc::S_IRUSR | nc::S_IWUSR) as i32;
/// let ret = unsafe { nc::msgget(key, flags) };
/// assert!(ret.is_ok());
/// let msq_id = ret.unwrap();
///
/// let mut buf = nc::msqid_ds_t::default();
/// let ret = unsafe { nc::msgctl(msq_id, nc::IPC_RMID, &mut buf) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn msgget(key: key_t, msg_flag: i32) -> Result<i32, Errno> {
    let key = key as usize;
    let msg_flag = msg_flag as usize;
    syscall2(SYS_MSGGET, key, msg_flag).map(|ret| ret as i32)
}
