/// Generate universally unique identifiers
pub unsafe fn uuidgen(store: &mut [uuid_t]) -> Result<(), Errno> {
    let store_ptr = store.as_mut_ptr() as usize;
    let count = store.len();
    syscall2(SYS_UUIDGEN, store_ptr, count).map(drop)
}
