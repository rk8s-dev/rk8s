/// Move individual pages of a process to another node
pub unsafe fn move_pages(
    pid: pid_t,
    nr_pages: usize,
    pages: usize,
    nodes: *const i32,
    status: Option<&mut i32>,
    flags: u32,
) -> Result<(), Errno> {
    let pid = pid as usize;
    let nodes_ptr = nodes as usize;
    let status = status.map_or(core::ptr::null_mut::<i32>() as usize, |status| {
        status as *mut i32 as usize
    });
    // NOTE(Shaohua): Type of flags is i32 in kernel.
    let flags = flags as usize;
    syscall6(
        SYS_MOVE_PAGES,
        pid,
        nr_pages,
        pages,
        nodes_ptr,
        status,
        flags,
    )
    .map(drop)
}
