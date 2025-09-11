pub unsafe fn __mq_timedreceive50(
    mqdes: mqd_t,
    msg: &mut [u8],
    msg_len: usize,
    msg_prio: &mut u32,
    abs_timeout: &timespec_t,
) -> Result<ssize_t, Errno> {
    let mqdes = mqdes as usize;
    let msg = CString::new(msg);
    let msg_ptr = msg.as_ptr() as usize;
    let msg_prio = msg_prio as *mut u32 as usize;
    let abs_timeout_ptr = abs_timeout as *const timespec_t as usize;
    syscall5(
        SYS___MQ_TIMEDRECEIVE50,
        mqdes,
        msg_ptr,
        msg_len,
        msg_prio,
        abs_timeout_ptr,
    )
    .map(|ret| ret as ssize_t)
}
