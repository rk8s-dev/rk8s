/// Receive a message from a message queue (realtime)
pub unsafe fn mq_receive(
    mqdes: mqd_t,
    msg: &mut [u8],
    msg_len: usize,
    msg_prio: &mut u32,
) -> Result<ssize_t, Errno> {
    let mqdes = mqdes as usize;
    let msg = CString::new(msg);
    let msg_ptr = msg.as_ptr() as usize;
    let msg_prio = msg_prio as *mut u32 as usize;
    syscall4(SYS_MQ_RECEIVE, mqdes, msg_ptr, msg_len, msg_prio).map(|ret| ret as ssize_t)
}
