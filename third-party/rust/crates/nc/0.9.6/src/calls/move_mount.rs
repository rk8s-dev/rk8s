/// Move a mount from one place to another.
///
/// In combination with `fsopen()/fsmount()` this is used to install a new mount
/// and in combination with `open_tree(OPEN_TREE_CLONE [| AT_RECURSIVE])`
/// it can be used to copy a mount subtree.
///
/// Note the flags value is a combination of `MOVE_MOUNT_*` flags.
pub unsafe fn move_mount<P: AsRef<Path>>(
    from_dfd: i32,
    from_pathname: P,
    to_dfd: i32,
    to_pathname: P,
    flags: u32,
) -> Result<i32, Errno> {
    let from_dfd = from_dfd as usize;
    let from_pathname = CString::new(from_pathname.as_ref());
    let from_pathname_ptr = from_pathname.as_ptr() as usize;
    let to_dfd = to_dfd as usize;
    let to_pathname = CString::new(to_pathname.as_ref());
    let to_pathname_ptr = to_pathname.as_ptr() as usize;
    let flags = flags as usize;
    syscall5(
        SYS_MOVE_MOUNT,
        from_dfd,
        from_pathname_ptr,
        to_dfd,
        to_pathname_ptr,
        flags,
    )
    .map(|ret| ret as i32)
}
