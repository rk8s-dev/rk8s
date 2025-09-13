/// Allocates a System V shared memory segment.
///
/// # Examples
///
/// ```
/// let size = 4 * nc::PAGE_SIZE;
/// let flags = nc::IPC_CREAT | nc::IPC_EXCL | 0o600;
/// let ret = unsafe { nc::shmget(nc::IPC_PRIVATE, size, flags) };
/// assert!(ret.is_ok());
/// let shmid = ret.unwrap();
/// let mut buf = nc::shmid_ds_t::default();
/// let ret = unsafe { nc::shmctl(shmid, nc::IPC_RMID, &mut buf) };
/// assert!(ret.is_ok());
/// ```
pub unsafe fn shmget(key: key_t, size: size_t, shm_flag: i32) -> Result<i32, Errno> {
    let key = key as usize;
    let shm_flag = shm_flag as usize;
    syscall3(SYS_SHMGET, key, size, shm_flag).map(|ret| ret as i32)
}
