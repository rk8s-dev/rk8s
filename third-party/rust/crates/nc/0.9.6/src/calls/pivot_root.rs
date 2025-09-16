/// Change the root filesystem.
pub unsafe fn pivot_root<P: AsRef<Path>>(new_root: P, put_old: P) -> Result<(), Errno> {
    let new_root = CString::new(new_root.as_ref());
    let new_root_ptr = new_root.as_ptr() as usize;
    let put_old = CString::new(put_old.as_ref());
    let put_old_ptr = put_old.as_ptr() as usize;
    syscall2(SYS_PIVOT_ROOT, new_root_ptr, put_old_ptr).map(drop)
}
