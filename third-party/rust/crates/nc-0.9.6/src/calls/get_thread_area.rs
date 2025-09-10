/// Get thread-local storage information.
pub unsafe fn get_thread_area(info: &mut user_desc_t) -> Result<(), Errno> {
    let info_ptr = info as *mut user_desc_t as usize;
    syscall1(SYS_GET_THREAD_AREA, info_ptr).map(drop)
}
