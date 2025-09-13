/// Opens (or optionally creates) a POSIX shared memory object named `path`.
pub unsafe fn shm_open2<P: AsRef<Path>>(
    path: P,
    flags: i32,
    mode: mode_t,
    shmflags: i32,
    fcaps: &mut filecaps_t,
    name: P,
) -> Result<(), Errno> {
    let path = CString::new(path.as_ref());
    let path_ptr = path.as_ptr() as usize;
    let flags = flags as usize;
    let mode = mode as usize;
    let shmflags = shmflags as usize;
    let fcaps_ptr = fcaps as *mut filecaps_t as usize;
    let name = CString::new(name.as_ref());
    let name_ptr = name.as_ptr() as usize;
    syscall6(
        SYS_SHM_OPEN2,
        path_ptr,
        flags,
        mode,
        shmflags,
        fcaps_ptr,
        name_ptr,
    )
    .map(drop)
}
