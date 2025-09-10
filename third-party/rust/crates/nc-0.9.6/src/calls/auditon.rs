/// Configure system audit parameters
pub unsafe fn auditon(cmd: i32, data: &[u8]) -> Result<(), Errno> {
    let cmd = cmd as usize;
    let data_ptr = data.as_ptr() as usize;
    let length = data.len();
    syscall3(SYS_AUDITON, cmd, data_ptr, length).map(drop)
}
