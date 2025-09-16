/// Open a message queue (REALTIME)
pub unsafe fn kmq_open<P: AsRef<Path>>(
    path: P,
    flags: i32,
    mode: mode_t,
    attr: &mq_attr_t,
) -> Result<i32, Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let flags = flags as usize;
    let mode = mode as usize;
    let attr_ptr = attr as *const mq_attr_t as usize;
    syscall4(SYS_KMQ_OPEN, path_ptr, flags, mode, attr_ptr).map(|ret| ret as i32)
}
