/// Add, remove, or modify an fanotify mark on a filesystem object
pub unsafe fn fanotify_mark<P: AsRef<Path>>(
    fanotify_fd: i32,
    flags: u32,
    mask: u64,
    dir_fd: i32,
    filename: Option<P>,
) -> Result<(), Errno> {
    let fanotify_fd = fanotify_fd as usize;
    let flags = flags as usize;
    let mask = mask as usize;
    let dir_fd = dir_fd as usize;
    let filename = filename.map(|filename| CString::new(filename.as_ref()));
    let filename_ptr = filename.map_or(core::ptr::null::<u8>() as usize, |filename| {
        filename.as_ptr() as usize
    });
    syscall5(
        SYS_FANOTIFY_MARK,
        fanotify_fd,
        flags,
        mask,
        dir_fd,
        filename_ptr,
    )
    .map(drop)
}
