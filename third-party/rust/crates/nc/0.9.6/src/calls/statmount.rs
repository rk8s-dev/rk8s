pub unsafe fn statmount(
    req: &mnt_id_req_t,
    buf: &mut [statmount_t],
    flags: u32,
) -> Result<(), Errno> {
    let req_ptr = req as *const mnt_id_req_t as usize;
    let buf_ptr = buf.as_mut_ptr() as usize;
    let buf_size = buf.len();
    let flags = flags as usize;
    syscall4(SYS_STATMOUNT, req_ptr, buf_ptr, buf_size, flags).map(drop)
}
