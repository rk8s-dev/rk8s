/// Set capabilities of thread.
pub unsafe fn capset(hdrp: &mut cap_user_header_t, data: &cap_user_data_t) -> Result<(), Errno> {
    let hdrp_ptr = hdrp as *mut cap_user_header_t as usize;
    let data_ptr = data as *const cap_user_data_t as usize;
    syscall2(SYS_CAPSET, hdrp_ptr, data_ptr).map(drop)
}
