/// Attach the System V shared memory segment.
///
/// # Examples
///
/// ```
/// let size = 4 * nc::PAGE_SIZE;
/// let flags = nc::IPC_CREAT | nc::IPC_EXCL | 0o600;
/// let ret = unsafe { nc::shmget(nc::IPC_PRIVATE, size, flags) };
/// assert!(ret.is_ok());
/// let shmid = ret.unwrap();
///
/// let ret = unsafe { nc::shmat(shmid, None, 0) };
/// assert!(ret.is_ok());
/// let addr: *const std::ffi::c_void = ret.unwrap();
///
/// let mut buf = nc::shmid_ds_t::default();
/// let ret = unsafe { nc::shmctl(shmid, nc::IPC_STAT, &mut buf) };
/// assert!(ret.is_ok());
///
/// let ret = unsafe { nc::shmdt(addr) };
/// assert!(ret.is_ok());
///
/// let ret = unsafe { nc::shmctl(shmid, nc::IPC_RMID, &mut buf) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn shmat(
    shmid: i32,
    shm_addr: Option<*const core::ffi::c_void>,
    shm_flag: i32,
) -> Result<*const core::ffi::c_void, Errno> {
    let shmid = shmid as usize;
    let shm_addr = shm_addr.map_or(core::ptr::null::<u8>() as usize, |shm_addr| {
        shm_addr as usize
    });
    let shm_flag = shm_flag as usize;
    syscall3(SYS_SHMAT, shmid, shm_addr, shm_flag).map(|ret| ret as *const core::ffi::c_void)
}
