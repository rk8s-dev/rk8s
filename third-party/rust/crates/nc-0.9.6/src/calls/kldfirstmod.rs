/// Returns first module id from the kld file specified.
pub unsafe fn kldfirstmod(file_id: i32) -> Result<i32, Errno> {
    let file_id = file_id as usize;
    syscall1(SYS_KLDFIRSTMOD, file_id).map(|ret| ret as i32)
}
