/// System V message control operations.
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
pub unsafe fn msgctl(msq_id: i32, cmd: i32, buf: &mut msqid_ds_t) -> Result<i32, Errno> {
    let msq_id = msq_id as usize;
    let cmd = cmd as usize;
    let buf_ptr = buf as *mut msqid_ds_t as usize;
    syscall3(SYS_MSGCTL, msq_id, cmd, buf_ptr).map(|ret| ret as i32)
}
