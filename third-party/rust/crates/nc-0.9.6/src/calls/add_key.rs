/// Add a key to the kernel's key management facility.
pub unsafe fn add_key<P: AsRef<Path>>(
    type_: P,
    description: P,
    payload: *const core::ffi::c_void,
    plen: size_t,
    dest_keyring: key_serial_t,
) -> Result<key_serial_t, Errno> {
    let type_ = CString::new(type_.as_ref());
    let type_ptr = type_.as_ptr() as usize;
    let payload = payload as usize;
    let description = CString::new(description.as_ref());
    let description_ptr = description.as_ptr() as usize;
    let dest_keyring = dest_keyring as usize;
    syscall5(
        SYS_ADD_KEY,
        type_ptr,
        description_ptr,
        payload,
        plen,
        dest_keyring,
    )
    .map(|ret| ret as key_serial_t)
}
