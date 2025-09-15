pub unsafe fn freebsd10__umtx_lock(umtx: &mut umtx_t) -> Result<(), Errno> {
    let umtx_ptr = umtx as *mut umtx_t as usize;
    syscall1(SYS_FREEBSD10__UMTX_LOCK, umtx_ptr).map(drop)
}
