/// Set message queue attributes (REALTIME)
pub unsafe fn kmq_setattr(
    mqd: i32,
    attr: &mq_attr_t,
    old_attr: &mut mq_attr_t,
) -> Result<(), Errno> {
    let mqd = mqd as usize;
    let attr_ptr = attr as *const mq_attr_t as usize;
    let old_attr_ptr = old_attr as *mut mq_attr_t as usize;
    syscall3(SYS_KMQ_SETATTR, mqd, attr_ptr, old_attr_ptr).map(drop)
}
