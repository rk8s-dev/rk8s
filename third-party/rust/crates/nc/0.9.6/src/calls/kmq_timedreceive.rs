/// Receive a message from message queue (REALTIME)
pub unsafe fn kmq_timedreceive(
    mqd: i32,
    msg: &mut [u8],
    msg_len: usize,
    msg_prio: u32,
    abs_timeout: &timespec_t,
) -> Result<ssize_t, Errno> {
    let mqd = mqd as usize;
    let msg_ptr = msg.as_mut_ptr() as usize;
    let msg_prio = msg_prio as usize;
    let abs_timeout_ptr = abs_timeout as *const timespec_t as usize;
    syscall5(
        SYS_KMQ_TIMEDRECEIVE,
        mqd,
        msg_ptr,
        msg_len,
        msg_prio,
        abs_timeout_ptr,
    )
    .map(|val| val as ssize_t)
}
