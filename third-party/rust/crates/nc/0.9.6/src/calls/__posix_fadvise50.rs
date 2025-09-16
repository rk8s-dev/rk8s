/// Give advice about use of file data
pub unsafe fn __posix_fadvise50(
    fd: i32,
    pad: i32,
    offset: off_t,
    len: off_t,
    advice: i32,
) -> Result<(), Errno> {
    let fd = fd as usize;
    let pad = pad as usize;
    let offset = offset as usize;
    let len = len as usize;
    let advice = advice as usize;
    syscall5(SYS___POSIX_FADVISE50, fd, pad, offset, len, advice).map(drop)
}
