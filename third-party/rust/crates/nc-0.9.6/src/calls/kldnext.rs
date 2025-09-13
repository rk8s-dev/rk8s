/// Returns the file id of the next kld file.
pub unsafe fn kldnext(file_id: i32) -> Result<i32, Errno> {
    let file_id = file_id as usize;
    syscall1(SYS_KLDNEXT, file_id).map(|ret| ret as i32)
}
