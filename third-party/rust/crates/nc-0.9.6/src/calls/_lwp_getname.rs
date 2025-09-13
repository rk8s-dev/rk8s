pub unsafe fn _lwp_getname(target: lwpid_t, buf: &mut [u8]) -> Result<i32, Errno> {
    let target = target as usize;
    let buf_ptr = buf.as_mut_ptr() as usize;
    let buf_len = buf.len();
    syscall3(SYS__LWP_GETNAME, target, buf_ptr, buf_len).map(|val| val as i32)
}
