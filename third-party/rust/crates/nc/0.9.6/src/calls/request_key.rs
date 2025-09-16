/// Request a key from kernel's key management facility.
pub unsafe fn request_key<P: AsRef<Path>>(
    type_: P,
    description: P,
    callout_info: Option<P>,
    dest_keyring: key_serial_t,
) -> Result<key_serial_t, Errno> {
    let type_ = CString::new(type_.as_ref());
    let type_ptr = type_.as_ptr() as usize;
    let description = CString::new(description.as_ref());
    let description_ptr = description.as_ptr() as usize;
    let callout_info = callout_info.map(|callout_info| CString::new(callout_info.as_ref()));
    let callout_info_ptr = callout_info.map_or(core::ptr::null::<u8>() as usize, |callout_info| {
        callout_info.as_ptr() as usize
    });
    let dest_keyring = dest_keyring as usize;
    syscall4(
        SYS_REQUEST_KEY,
        type_ptr,
        description_ptr,
        callout_info_ptr,
        dest_keyring,
    )
    .map(|ret| ret as key_serial_t)
}
