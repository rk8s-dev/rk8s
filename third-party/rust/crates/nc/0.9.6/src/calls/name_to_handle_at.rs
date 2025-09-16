/// Obtain handle for a filename
pub unsafe fn name_to_handle_at<P: AsRef<Path>>(
    dfd: i32,
    filename: P,
    handle: &mut file_handle_t,
    mount_id: &mut i32,
    flags: i32,
) -> Result<(), Errno> {
    let dfd = dfd as usize;
    let filename = CString::new(filename.as_ref());
    let filename_ptr = filename.as_ptr() as usize;
    let handle_ptr = handle as *mut file_handle_t as usize;
    let mount_id_ptr = mount_id as *mut i32 as usize;
    let flags = flags as usize;
    syscall5(
        SYS_NAME_TO_HANDLE_AT,
        dfd,
        filename_ptr,
        handle_ptr,
        mount_id_ptr,
        flags,
    )
    .map(drop)
}
