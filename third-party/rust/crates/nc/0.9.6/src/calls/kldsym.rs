/// Look up address by symbol name in a kld file.
pub unsafe fn kldsym(file_id: i32, cmd: i32, data: usize) -> Result<(), Errno> {
    let file_id = file_id as usize;
    let cmd = cmd as usize;
    syscall3(SYS_KLDSYM, file_id, cmd, data).map(drop)
}
