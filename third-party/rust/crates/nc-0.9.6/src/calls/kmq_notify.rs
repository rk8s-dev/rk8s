/// Notify process that a message is available (REALTIME)
pub unsafe fn kmq_notify(mqd: i32, event: &sigevent_t) -> Result<(), Errno> {
    let mqd = mqd as usize;
    let event_ptr = event as *const sigevent_t as usize;
    syscall2(SYS_KMQ_NOTIFY, mqd, event_ptr).map(drop)
}
