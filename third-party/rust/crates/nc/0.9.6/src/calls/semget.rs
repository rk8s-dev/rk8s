/// Get a System V semphore set identifier.
///
/// # Examples
///
/// ```
/// use std::thread;
/// use std::time::Duration;
///
/// pub struct BinarySemaphore {
///     key: i32,
///     semid: i32,
///     is_producer: bool,
/// }
///
/// impl BinarySemaphore {
///     const SEM_NUM: i32 = 0;
///     const FIRST_SEM: u16 = 0;
///     const SEM_MAX_VAL: i32 = 1;
///     const SEM_OP_RESERVE: i16 = -1;
///     const SEM_OP_RELEASE: i16 = 1;
///
///     pub fn new(key: i32, is_producer: bool) -> Result<Self, nc::Errno> {
///         let semid =
///             unsafe { nc::semget(key, 1, nc::IPC_CREAT | (nc::S_IRUSR | nc::S_IWUSR) as i32)? };
///
///         let ret = unsafe {
///             let arg = Self::SEM_MAX_VAL as usize;
///             nc::semctl(semid, Self::SEM_NUM, nc::SETVAL, arg)
///         };
///
///         if let Err(errno) = ret {
///             let _ret = unsafe { nc::semctl(semid, nc::IPC_RMID, 0, 0) };
///             return Err(errno);
///         }
///
///         Ok(Self {
///             key,
///             semid,
///             is_producer,
///         })
///     }
///
///     pub fn reserve(&mut self) -> Result<(), nc::Errno> {
///         let mut ops = [nc::sembuf_t {
///             sem_num: Self::FIRST_SEM,
///             sem_op: Self::SEM_OP_RESERVE,
///             sem_flg: 0,
///         }];
///
///         loop {
///             let ret = unsafe { nc::semop(self.semid, &mut ops) };
///             match ret {
///                 Ok(_) => return Ok(()),
///                 Err(nc::EINTR) => continue,
///                 Err(errno) => return Err(errno),
///             }
///         }
///     }
///
///     pub fn release(&mut self) -> Result<(), nc::Errno> {
///         let mut ops = [nc::sembuf_t {
///             sem_num: Self::FIRST_SEM,
///             sem_op: Self::SEM_OP_RELEASE,
///             sem_flg: 0,
///         }];
///         unsafe { nc::semop(self.semid, &mut ops) }
///     }
///
///     #[must_use]
///     #[inline]
///     pub const fn key(&self) -> i32 {
///         self.key
///     }
///
///     #[must_use]
///     #[inline]
///     pub const fn is_producer(&self) -> bool {
///         self.is_producer
///     }
/// }
///
/// impl Drop for BinarySemaphore {
///     fn drop(&mut self) {
///         if self.is_producer {
///             let ret = unsafe { nc::semctl(self.semid, nc::IPC_RMID, 0, 0) };
///             assert!(ret.is_ok());
///         }
///     }
/// }
///
/// const KEY_ID: i32 = 0x1235;
/// let ret = BinarySemaphore::new(KEY_ID, true);
/// if let Err(errno) = ret {
///     eprintln!("sem init failed: {}", nc::strerror(errno));
///     return;
/// }
/// let mut sem = ret.unwrap();
///
/// // child thread as consumer
/// let handle = thread::spawn(|| {
///     let mut sem = BinarySemaphore::new(KEY_ID, false).unwrap();
///     for _ in 0..5 {
///         if let Err(errno) = sem.reserve() {
///             eprintln!("[worker ]sem reserve failed: {}", nc::strerror(errno));
///             break;
///         }
///         println!("[worker] wait for 100 millis");
///         thread::sleep(Duration::from_millis(100));
///         if let Err(errno) = sem.release() {
///             eprintln!("[worker] sem release failed: {}", nc::strerror(errno));
///         }
///     }
/// });
///
/// // parent thread as producer
/// for _ in 0..5 {
///     if let Err(errno) = sem.reserve() {
///         eprintln!("[worker ]sem reserve failed: {}", nc::strerror(errno));
///         break;
///     }
///     println!("[main] wait for 200 millis");
///     thread::sleep(Duration::from_millis(200));
///     if let Err(errno) = sem.release() {
///         eprintln!("[main] sem release failed: {}", nc::strerror(errno));
///     }
/// }
///
/// let _ = handle.join();
/// ```
pub unsafe fn semget(key: key_t, nsems: i32, sem_flag: i32) -> Result<i32, Errno> {
    let key = key as usize;
    let nsems = nsems as usize;
    let sem_flag = sem_flag as usize;
    syscall3(SYS_SEMGET, key, nsems, sem_flag).map(|ret| ret as i32)
}
