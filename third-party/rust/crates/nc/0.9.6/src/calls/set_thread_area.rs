/// Set thread-local storage information.
pub unsafe fn set_thread_area(info: &mut user_desc_t) -> Result<(), Errno> {
    let info_ptr = info as *mut user_desc_t as usize;
    syscall1(SYS_SET_THREAD_AREA, info_ptr).map(drop)
}
