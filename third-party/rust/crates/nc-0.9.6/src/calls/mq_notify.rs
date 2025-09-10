/// Register for notification when a message is available
pub unsafe fn mq_notify(mqdes: mqd_t, notification: Option<&sigevent_t>) -> Result<(), Errno> {
    let mqdes = mqdes as usize;
    let notification_ptr = notification
        .map_or(core::ptr::null::<sigevent_t>() as usize, |notification| {
            notification as *const sigevent_t as usize
        });
    syscall2(SYS_MQ_NOTIFY, mqdes, notification_ptr).map(drop)
}
