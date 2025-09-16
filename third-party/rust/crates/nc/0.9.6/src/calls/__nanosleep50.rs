/// High resolution sleep.
pub unsafe fn __nanosleep50(req: &timespec_t, rem: Option<&mut timespec_t>) -> Result<(), Errno> {
    let req_ptr = req as *const timespec_t as usize;
    let rem_ptr = rem.map_or(0, |rem| rem as *mut timespec_t as usize);
    syscall2(SYS___NANOSLEEP50, req_ptr, rem_ptr).map(drop)
}
