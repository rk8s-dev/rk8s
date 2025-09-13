/// Send message to a message queue (realtime)
pub unsafe fn mq_send(
    mqdes: mqd_t,
    msg: &[u8],
    msg_len: usize,
    msg_prio: u32,
) -> Result<(), Errno> {
    let mqdes = mqdes as usize;
    let msg = CString::new(msg);
    let msg_ptr = msg.as_ptr() as usize;
    let msg_prio = msg_prio as usize;
    syscall4(SYS_MQ_SEND, mqdes, msg_ptr, msg_len, msg_prio).map(drop)
}
