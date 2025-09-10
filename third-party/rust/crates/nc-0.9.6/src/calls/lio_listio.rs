/// List directed I/O (REALTIME)
pub unsafe fn lio_listio(
    mode: i32,
    acb_list: &mut [aiocb_t],
    sig: &mut sigevent_t,
) -> Result<(), Errno> {
    let mode = mode as usize;
    let acb_list_ptr = acb_list.as_mut_ptr() as usize;
    let acb_list_len = acb_list.len();
    let sig_ptr = sig as *mut sigevent_t as usize;
    syscall4(SYS_LIO_LISTIO, mode, acb_list_ptr, acb_list_len, sig_ptr).map(drop)
}
