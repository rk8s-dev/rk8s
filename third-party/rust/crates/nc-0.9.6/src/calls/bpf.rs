/// Perform a command on an extended BPF map or program
pub unsafe fn bpf(cmd: i32, attr: &mut bpf_attr_t, size: u32) -> Result<i32, Errno> {
    let cmd = cmd as usize;
    let attr_ptr = attr as *mut bpf_attr_t as usize;
    let size = size as usize;
    syscall3(SYS_BPF, cmd, attr_ptr, size).map(|ret| ret as i32)
}
