/// Close a message queue (realtime)
pub unsafe fn mq_close(mqdes: mqd_t) -> Result<(), Errno> {
    let mqdes = mqdes as usize;
    syscall1(SYS_MQ_CLOSE, mqdes).map(drop)
}
