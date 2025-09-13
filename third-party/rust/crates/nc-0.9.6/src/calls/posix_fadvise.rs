/// Give advice about use of file data
pub unsafe fn posix_fadvise(
    fd: i32,
    offset: loff_t,
    len: size_t,
    advice: i32,
) -> Result<(), Errno> {
    let fd = fd as usize;
    let offset = offset as usize;
    let advice = advice as usize;
    syscall4(SYS_POSIX_FADVISE, fd, offset, len, advice).map(drop)
}
