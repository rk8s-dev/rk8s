use fslock::LockFile;
#[cfg(feature = "logging")]
use log::debug;
use std::{
    env,
    fs::{self, File},
    io::{Read, Write},
    path::Path,
    thread,
    time::Duration,
};

pub(crate) struct Lock {
    lockfile: LockFile,
    pub(crate) parallel_count: u32,
    path: String,
}

impl Lock {
    // Can't use the same file as fslock truncates it
    fn gen_count_file(path: &str) -> String {
        format!("{}-count", path)
    }

    fn read_parallel_count(path: &str) -> u32 {
        let parallel_count = match File::open(Lock::gen_count_file(path)) {
            Ok(mut file) => {
                let mut count_buf = [0; 4];
                match file.read_exact(&mut count_buf) {
                    Ok(_) => u32::from_ne_bytes(count_buf),
                    Err(_err) => {
                        #[cfg(feature = "logging")]
                        debug!("Error loading count file: {}", _err);
                        0u32
                    }
                }
            }
            Err(_) => 0,
        };

        #[cfg(feature = "logging")]
        debug!("Parallel count for {:?} is {}", path, parallel_count);
        parallel_count
    }

    pub(crate) fn new(path: &str) -> Lock {
        if !Path::new(path).exists() {
            fs::write(path, "").unwrap_or_else(|_| panic!("Lock file path was {:?}", path))
        }
        let mut lockfile = LockFile::open(path).unwrap();

        #[cfg(feature = "logging")]
        debug!("Waiting on {:?}", path);

        lockfile.lock().unwrap();

        #[cfg(feature = "logging")]
        debug!("Locked for {:?}", path);

        Lock {
            lockfile,
            parallel_count: Lock::read_parallel_count(path),
            path: String::from(path),
        }
    }

    pub(crate) fn start_serial(self: &mut Lock) {
        loop {
            if self.parallel_count == 0 {
                return;
            }
            #[cfg(feature = "logging")]
            debug!("Waiting because parallel count is {}", self.parallel_count);
            // unlock here is safe because we re-lock before returning
            self.unlock();
            thread::sleep(Duration::from_secs(1));
            self.lockfile.lock().unwrap();
            #[cfg(feature = "logging")]
            debug!("Locked for {:?}", self.path);
            self.parallel_count = Lock::read_parallel_count(&self.path)
        }
    }

    fn unlock(self: &mut Lock) {
        #[cfg(feature = "logging")]
        debug!("Unlocking {}", self.path);
        self.lockfile.unlock().unwrap();
    }

    pub(crate) fn end_serial(mut self: Lock) {
        self.unlock();
    }

    fn write_parallel(self: &Lock) {
        let mut file = File::create(&Lock::gen_count_file(&self.path)).unwrap();
        file.write_all(&self.parallel_count.to_ne_bytes()).unwrap();
    }

    pub(crate) fn start_parallel(self: &mut Lock) {
        self.parallel_count += 1;
        self.write_parallel();
        self.unlock();
    }

    pub(crate) fn end_parallel(mut self: Lock) {
        assert!(self.parallel_count > 0);
        self.parallel_count -= 1;
        self.write_parallel();
        self.unlock();
    }
}

pub(crate) fn path_for_name(name: &str) -> String {
    let mut pathbuf = env::temp_dir();
    pathbuf.push(format!("serial-test-{}", name));
    pathbuf.into_os_string().into_string().unwrap()
}

fn make_lock_for_name_and_path(name: &str, path: Option<&str>) -> Lock {
    if let Some(opt_path) = path {
        Lock::new(opt_path)
    } else {
        let default_path = path_for_name(name);
        Lock::new(&default_path)
    }
}

pub(crate) fn get_locks(names: &Vec<&str>, path: Option<&str>) -> Vec<Lock> {
    if names.len() > 1 && path.is_some() {
        panic!("Can't do file_parallel with both more than one name _and_ a specific path");
    }
    names
        .iter()
        .map(|name| make_lock_for_name_and_path(name, path))
        .collect::<Vec<_>>()
}
