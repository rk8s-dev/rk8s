/// Manipulate kernel environment.
pub unsafe fn kenv(action: i32, name: &str, value: Option<&mut [u8]>) -> Result<i32, Errno> {
    let action = action as usize;
    let name = CString::new(name);
    let name_ptr = name.as_ptr() as usize;
    let value_ptr = value.map_or(0, |value| value.as_mut_ptr() as usize);
    syscall3(SYS_KENV, action, name_ptr, value_ptr).map(|val| val as i32)
}
