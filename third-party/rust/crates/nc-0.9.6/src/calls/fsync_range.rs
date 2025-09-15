/// Flush all modified in-core data refered by `fd` to disk.
pub unsafe fn fsync_range(fd: i32, how: i32, start: off_t, length: off_t) -> Result<(), Errno> {
    let fd = fd as usize;
    let how = how as usize;
    let start = start as usize;
    let length = length as usize;
    syscall4(SYS_FSYNC_RANGE, fd, how, start, length).map(drop)
}
