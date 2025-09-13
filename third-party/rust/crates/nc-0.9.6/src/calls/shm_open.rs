/// Opens (or optionally creates) a POSIX shared memory object named `path`.
pub unsafe fn shm_open<P: AsRef<Path>>(name: P, flags: i32, mode: i32) -> Result<i32, Errno> {
    let name = CString::new(name.as_ref());
    let name_ptr = name.as_ptr() as usize;
    let flags = flags as usize;
    let mode = mode as usize;
    syscall3(SYS_SHM_OPEN, name_ptr, flags, mode).map(|val| val as i32)
}
