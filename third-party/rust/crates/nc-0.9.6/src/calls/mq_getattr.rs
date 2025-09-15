/// Get message queue attributes (realtime)
pub unsafe fn mq_getattr(mqdes: mqd_t, attr: Option<&mut mq_attr_t>) -> Result<(), Errno> {
    let mqdes = mqdes as usize;
    let attr_ptr = attr.map_or(0, |attr| attr as *mut mq_attr_t as usize);
    syscall2(SYS_MQ_GETATTR, mqdes, attr_ptr).map(drop)
}
