#![no_main]

use std::ffi::CString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink as model_symlink};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use etcd_client::{Client as RawEtcdClient, DeleteOptions};
use libfuzzer_sys::fuzz_target;
use slayerfs::{
    CacheConfig, ChunkLayout, ClientOptions, Config, DatabaseConfig, DatabaseMetaStore,
    DatabaseType, EtcdMetaStore, LocalFsBackend, MetaClient, MetaStore, ObjectBlockStore,
    ObjectClient, SetAttrFlags, SetAttrRequest, VFS, VfsFileAttr, VfsFileType,
};
use tokio::runtime::Builder;
use tokio::task::JoinSet;

const LAYOUT: ChunkLayout = ChunkLayout {
    chunk_size: 128,
    block_size: 64,
};

const MAX_FDS: usize = 16;
const MAX_READDIR_ENTRIES: usize = 50;

const DIR_NAMES: [&str; 4] = ["d0", "d1", "d2", "d3"];
const SUBDIR_NAMES: [&str; 2] = ["sub0", "sub1"];
const HARDLINK_NAMES: [&str; 4] = ["h0", "h1", "h2", "h3"];
const SYMLINK_NAMES: [&str; 4] = ["s0", "s1", "s2", "s3"];
const RW_NAMES: [&str; 10] = ["f0", "f1", "f2", "f3", "f4", "f5", "h0", "h1", "h2", "h3"];
const UNLINK_NAMES: [&str; 14] = [
    "f0", "f1", "f2", "f3", "f4", "f5", "h0", "h1", "h2", "h3", "s0", "s1", "s2", "s3",
];
const STAT_NAMES: [&str; 16] = [
    "f0", "f1", "f2", "f3", "f4", "f5", "h0", "h1", "h2", "h3", "s0", "s1", "s2", "s3", "sub0",
    "sub1",
];
const ALL_NAMES: [&str; 16] = [
    "f0", "f1", "f2", "f3", "f4", "f5", "h0", "h1", "h2", "h3", "s0", "s1", "s2", "s3", "sub0",
    "sub1",
];

type SlayerVfs = VFS<ObjectBlockStore<LocalFsBackend>, MetaClient<Arc<dyn MetaStore>>>;

#[repr(u8)]
#[derive(Debug, Clone, Copy)]
enum OpKind {
    Open = 0,
    Close = 1,
    Lseek = 2,
    ReadFd = 3,
    WriteFd = 4,
    PRead = 5,
    PWrite = 6,
    Create = 7,
    Unlink = 8,
    Mkdir = 9,
    Rmdir = 10,
    Readdir = 11,
    Readlink = 12,
    Stat = 13,
    Lstat = 14,
    Fstat = 15,
    Utimens = 16,
    Ftruncate = 17,
    Fsync = 18,
    Fdatasync = 19,
    Symlink = 20,
    Link = 21,
    PurgeDir = 22,
    Chmod = 23,
    Fchmod = 24,
    Rename = 25,
}

impl From<u8> for OpKind {
    fn from(value: u8) -> Self {
        match value {
            0 => OpKind::Open,
            1 => OpKind::Close,
            2 => OpKind::Lseek,
            3 => OpKind::ReadFd,
            4 => OpKind::WriteFd,
            5 => OpKind::PRead,
            6 => OpKind::PWrite,
            7 => OpKind::Create,
            8 => OpKind::Unlink,
            9 => OpKind::Mkdir,
            10 => OpKind::Rmdir,
            11 => OpKind::Readdir,
            12 => OpKind::Readlink,
            13 => OpKind::Stat,
            14 => OpKind::Lstat,
            15 => OpKind::Fstat,
            16 => OpKind::Utimens,
            17 => OpKind::Ftruncate,
            18 => OpKind::Fsync,
            19 => OpKind::Fdatasync,
            20 => OpKind::Symlink,
            21 => OpKind::Link,
            22 => OpKind::PurgeDir,
            23 => OpKind::Chmod,
            24 => OpKind::Fchmod,
            25 => OpKind::Rename,
            _ => panic!("unknown op kind"),
        }
    }
}

impl OpKind {
    fn count() -> u8 {
        26
    }
}

#[derive(Debug, Clone, Copy)]
enum AccessMode {
    ReadOnly,
    WriteOnly,
    ReadWrite,
}

impl AccessMode {
    fn from_u8(v: u8) -> Self {
        match v % 3 {
            0 => AccessMode::ReadOnly,
            1 => AccessMode::WriteOnly,
            _ => AccessMode::ReadWrite,
        }
    }

    fn read(self) -> bool {
        matches!(self, AccessMode::ReadOnly | AccessMode::ReadWrite)
    }

    fn write(self) -> bool {
        matches!(self, AccessMode::WriteOnly | AccessMode::ReadWrite)
    }
}

#[derive(Debug, Clone, Copy)]
enum SeekWhence {
    Start,
    Current,
    End,
}

#[derive(Debug, Clone)]
enum Op {
    Open {
        slot: usize,
        path: String,
        mode: AccessMode,
        create: bool,
        excl: bool,
        trunc: bool,
        append: bool,
        nofollow: bool,
        directory: bool,
    },
    Close {
        slot: usize,
    },
    Lseek {
        slot: usize,
        whence: SeekWhence,
        delta: i64,
    },
    ReadFd {
        slot: usize,
        len: usize,
    },
    WriteFd {
        slot: usize,
        data: Vec<u8>,
    },
    PRead {
        slot: usize,
        off: u64,
        len: usize,
    },
    PWrite {
        slot: usize,
        off: u64,
        data: Vec<u8>,
    },
    Create {
        path: String,
        exclusive: bool,
    },
    Unlink {
        path: String,
    },
    Mkdir {
        path: String,
    },
    Rmdir {
        path: String,
    },
    Readdir {
        slot: usize,
    },
    Readlink {
        path: String,
    },
    Stat {
        path: String,
    },
    Lstat {
        path: String,
    },
    Fstat {
        slot: usize,
    },
    Utimens {
        path: String,
        atime_ns: i64,
        mtime_ns: i64,
        atime_now: bool,
        mtime_now: bool,
    },
    Ftruncate {
        slot: usize,
        size: u64,
    },
    Fsync {
        slot: usize,
    },
    Fdatasync {
        slot: usize,
    },
    Symlink {
        src: String,
        dst: String,
    },
    Link {
        src: String,
        dst: String,
    },
    Rename {
        src: String,
        dst: String,
    },
    PurgeDir {
        dir: String,
        recreate: bool,
    },
    Chmod {
        path: String,
        mode: u32,
    },
    Fchmod {
        slot: usize,
        mode: u32,
    },
}

#[derive(Clone, Copy)]
struct ByteCursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> ByteCursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn next_u8(&mut self) -> Option<u8> {
        let v = *self.data.get(self.pos)?;
        self.pos += 1;
        Some(v)
    }

    fn next_u16(&mut self) -> Option<u16> {
        let lo = self.next_u8()? as u16;
        let hi = self.next_u8()? as u16;
        Some(lo | (hi << 8))
    }

    fn next_u32(&mut self) -> Option<u32> {
        let lo = self.next_u16()? as u32;
        let hi = self.next_u16()? as u32;
        Some(lo | (hi << 16))
    }

    fn next_u64(&mut self) -> Option<u64> {
        let lo = self.next_u32()? as u64;
        let hi = self.next_u32()? as u64;
        Some(lo | (hi << 32))
    }

    fn choose<'b>(&mut self, values: &'b [&'b str]) -> Option<&'b str> {
        let idx = self.next_u8()? as usize;
        Some(values[idx % values.len()])
    }

    fn choose_with_dir(&mut self, values: &[&str]) -> Option<String> {
        Some(format!(
            "{}/{}",
            self.choose(&DIR_NAMES)?,
            self.choose(values)?
        ))
    }

    fn choose_dir_path(&mut self) -> Option<String> {
        if self.next_u8()? & 1 == 0 {
            Some(self.choose(&DIR_NAMES)?.to_string())
        } else {
            self.choose_with_dir(&SUBDIR_NAMES)
        }
    }

    fn choose_stat_path(&mut self) -> Option<String> {
        match self.next_u8()? % 3 {
            0 => self.choose_with_dir(&STAT_NAMES),
            1 => self.choose_dir_path(),
            _ => self.choose_with_dir(&RW_NAMES),
        }
    }

    fn choose_chmod_path(&mut self) -> Option<String> {
        if self.next_u8()? & 1 == 0 {
            self.choose_dir_path()
        } else {
            self.choose_with_dir(&RW_NAMES)
        }
    }

    fn next_mode(&mut self) -> Option<u32> {
        let raw = self.next_u16()? as u32;
        // Keep owner rwx so subsequent ops do not trivially fail on local permission checks.
        Some(0o700 | (raw & 0o077))
    }

    fn synth_data(seed: u64, len: usize) -> Vec<u8> {
        let mut x = seed ^ 0x9E37_79B9_7F4A_7C15;
        if x == 0 {
            x = 0xA5A5_A5A5_5A5A_5A5A;
        }

        let mut out = Vec::with_capacity(len);
        for _ in 0..len {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            out.push((x & 0xFF) as u8);
        }
        out
    }

    fn choose_op_kind(&mut self, has_file_fd: bool, has_dir_fd: bool) -> Option<OpKind> {
        let raw: OpKind = (self.next_u8()? % OpKind::count()).into();

        let adjusted = if !has_file_fd && !has_dir_fd {
            match raw {
                OpKind::Close
                | OpKind::Lseek
                | OpKind::ReadFd
                | OpKind::WriteFd
                | OpKind::PRead
                | OpKind::PWrite
                | OpKind::Readdir
                | OpKind::Fstat
                | OpKind::Ftruncate
                | OpKind::Fsync
                | OpKind::Fdatasync
                | OpKind::Fchmod => OpKind::Open,
                _ => raw,
            }
        } else if !has_file_fd {
            match raw {
                OpKind::Lseek
                | OpKind::ReadFd
                | OpKind::WriteFd
                | OpKind::PRead
                | OpKind::PWrite
                | OpKind::Ftruncate
                | OpKind::Fsync
                | OpKind::Fdatasync
                | OpKind::Fchmod => OpKind::Open,
                _ => raw,
            }
        } else if !has_dir_fd {
            match raw {
                OpKind::Readdir => OpKind::Open,
                _ => raw,
            }
        } else {
            raw
        };

        Some(adjusted)
    }

    fn next_op(&mut self, has_file_fd: bool, has_dir_fd: bool) -> Option<Op> {
        let limit = LAYOUT.chunk_size * 2;
        let typ = self.choose_op_kind(has_file_fd, has_dir_fd)?;

        Some(match typ {
            OpKind::Open => {
                let slot = self.next_u8()? as usize % MAX_FDS;
                let mode = AccessMode::from_u8(self.next_u8()?);
                let bits = self.next_u8()?;
                let create = (bits & 0b0000_0001) != 0;
                let excl = (bits & 0b0000_0010) != 0;
                let trunc = (bits & 0b0000_0100) != 0;
                let append = (bits & 0b0000_1000) != 0;
                let nofollow = (bits & 0b0001_0000) != 0;
                let directory = (bits & 0b0010_0000) != 0;

                let path = if directory {
                    self.choose_dir_path()?
                } else if nofollow && (self.next_u8()? & 1 == 0) {
                    self.choose_with_dir(&SYMLINK_NAMES)?
                } else {
                    self.choose_with_dir(&RW_NAMES)?
                };

                Op::Open {
                    slot,
                    path,
                    mode,
                    create,
                    excl,
                    trunc,
                    append,
                    nofollow,
                    directory,
                }
            }
            OpKind::Close => Op::Close {
                slot: self.next_u8()? as usize % MAX_FDS,
            },
            OpKind::Lseek => {
                let slot = self.next_u8()? as usize % MAX_FDS;
                let whence = match self.next_u8()? % 3 {
                    0 => SeekWhence::Start,
                    1 => SeekWhence::Current,
                    _ => SeekWhence::End,
                };
                let raw = (self.next_u64()? % limit) as i64;
                let delta = if self.next_u8()? & 1 == 0 { raw } else { -raw };
                Op::Lseek {
                    slot,
                    whence,
                    delta,
                }
            }
            OpKind::ReadFd => Op::ReadFd {
                slot: self.next_u8()? as usize % MAX_FDS,
                len: (self.next_u64()? % limit) as usize,
            },
            OpKind::WriteFd => {
                let slot = self.next_u8()? as usize % MAX_FDS;
                let len = 1 + (self.next_u64()? % limit) as usize;
                let seed = self.next_u64()?;
                let data = Self::synth_data(seed, len);
                Op::WriteFd { slot, data }
            }
            OpKind::PRead => Op::PRead {
                slot: self.next_u8()? as usize % MAX_FDS,
                off: self.next_u64()? % limit,
                len: (self.next_u64()? % limit) as usize,
            },
            OpKind::PWrite => {
                let slot = self.next_u8()? as usize % MAX_FDS;
                let off = self.next_u64()? % limit;
                let len = 1 + (self.next_u64()? % limit) as usize;
                let seed = self.next_u64()?;
                let data = Self::synth_data(seed, len);
                Op::PWrite { slot, off, data }
            }
            OpKind::Create => Op::Create {
                path: self.choose_with_dir(&RW_NAMES)?,
                exclusive: (self.next_u8()? & 1) != 0,
            },
            OpKind::Unlink => Op::Unlink {
                path: self.choose_with_dir(&UNLINK_NAMES)?,
            },
            OpKind::Mkdir => Op::Mkdir {
                path: self.choose_with_dir(&SUBDIR_NAMES)?,
            },
            OpKind::Rmdir => Op::Rmdir {
                path: self.choose_with_dir(&SUBDIR_NAMES)?,
            },
            OpKind::Readdir => Op::Readdir {
                slot: self.next_u8()? as usize % MAX_FDS,
            },
            OpKind::Readlink => Op::Readlink {
                path: self.choose_with_dir(&SYMLINK_NAMES)?,
            },
            OpKind::Stat => Op::Stat {
                path: self.choose_stat_path()?,
            },
            OpKind::Lstat => Op::Lstat {
                path: self.choose_stat_path()?,
            },
            OpKind::Fstat => Op::Fstat {
                slot: self.next_u8()? as usize % MAX_FDS,
            },
            OpKind::Utimens => {
                let path = self.choose_stat_path()?;
                let day_ns = 24_u64 * 60 * 60 * 1_000_000_000;
                let atime_ns = (self.next_u64()? % day_ns) as i64;
                let mtime_ns = (self.next_u64()? % day_ns) as i64;
                let bits = self.next_u8()?;
                Op::Utimens {
                    path,
                    atime_ns,
                    mtime_ns,
                    atime_now: (bits & 0b0000_0001) != 0,
                    mtime_now: (bits & 0b0000_0010) != 0,
                }
            }
            OpKind::Ftruncate => Op::Ftruncate {
                slot: self.next_u8()? as usize % MAX_FDS,
                size: self.next_u64()? % limit,
            },
            OpKind::Fsync => Op::Fsync {
                slot: self.next_u8()? as usize % MAX_FDS,
            },
            OpKind::Fdatasync => Op::Fdatasync {
                slot: self.next_u8()? as usize % MAX_FDS,
            },
            OpKind::Symlink => Op::Symlink {
                src: self.choose_with_dir(&RW_NAMES)?,
                dst: self.choose_with_dir(&SYMLINK_NAMES)?,
            },
            OpKind::Link => Op::Link {
                src: self.choose_with_dir(&RW_NAMES)?,
                dst: self.choose_with_dir(&HARDLINK_NAMES)?,
            },
            OpKind::Rename => Op::Rename {
                src: self.choose_with_dir(&UNLINK_NAMES)?,
                dst: self.choose_with_dir(&UNLINK_NAMES)?,
            },
            OpKind::PurgeDir => Op::PurgeDir {
                dir: self.choose(&DIR_NAMES)?.to_string(),
                recreate: (self.next_u8()? & 1) == 0,
            },
            OpKind::Chmod => Op::Chmod {
                path: self.choose_chmod_path()?,
                mode: self.next_mode()?,
            },
            OpKind::Fchmod => Op::Fchmod {
                slot: self.next_u8()? as usize % MAX_FDS,
                mode: self.next_mode()?,
            },
        })
    }
}

fn fs_path(name: &str) -> String {
    format!("/fuzz/{name}")
}

fn model_path(root: &Path, name: &str) -> PathBuf {
    root.join("fuzz").join(name)
}

fn bad_fd() -> io::Error {
    io::Error::from_raw_os_error(libc::EBADF)
}

fn loop_error() -> io::Error {
    io::Error::from_raw_os_error(libc::ELOOP)
}

fn is_notfound_like(err: &io::Error) -> bool {
    if err.kind() == io::ErrorKind::NotFound {
        return true;
    }

    let msg = err.to_string().to_ascii_lowercase();
    msg.contains("not found")
}

static ACTIVE_FUZZ_CLIENTS: AtomicUsize = AtomicUsize::new(1);

fn is_c2o_required_compare(op: &str) -> bool {
    matches!(
        op,
        "readfd" | "writefd" | "pread" | "pwrite" | "lseek" | "ftruncate"
    )
}

fn should_strict_compare(op: &str) -> bool {
    let clients = ACTIVE_FUZZ_CLIENTS.load(Ordering::Relaxed);
    clients <= 1 || is_c2o_required_compare(op)
}

fn compare_outcome<T, U>(op: &str, slayer: io::Result<T>, model: io::Result<U>) -> Option<(T, U)> {
    let strict = should_strict_compare(op);

    match (slayer, model) {
        (Ok(a), Ok(b)) => Some((a, b)),
        (Err(_), Err(_)) => None,
        (Ok(_), Err(_)) | (Err(_), Ok(_)) if !strict => None,
        (Ok(_), Err(e)) => panic!("{op}: slayerfs succeeded but model failed: {e:?}"),
        (Err(e), Ok(_)) => panic!("{op}: slayerfs failed but model succeeded: {e:?}"),
    }
}

fn model_kind(meta: &fs::Metadata) -> Option<VfsFileType> {
    if meta.file_type().is_file() {
        Some(VfsFileType::File)
    } else if meta.file_type().is_dir() {
        Some(VfsFileType::Dir)
    } else if meta.file_type().is_symlink() {
        Some(VfsFileType::Symlink)
    } else {
        None
    }
}

fn compare_attrs(ctx: &str, path: &str, slayer: &VfsFileAttr, model: &fs::Metadata) {
    let mk = model_kind(model).expect("unsupported model file type");
    assert_eq!(slayer.kind, mk, "{ctx}: kind mismatch at {path}");

    // Size can be transiently stale while write handles are still open; compare file
    // contents in verify_path/read operations instead.
    if slayer.kind == VfsFileType::File {
        assert_eq!(
            slayer.nlink as u64,
            model.nlink(),
            "{ctx}: nlink mismatch at {path}"
        );
    }
}

async fn slayer_read_file(vfs: &SlayerVfs, attr: &VfsFileAttr, len: usize) -> io::Result<Vec<u8>> {
    let fh = vfs
        .open(attr.ino, attr.clone(), true, false)
        .await
        .map_err(io::Error::from)?;
    let data_res = vfs.read(fh, 0, len).await.map_err(io::Error::from);
    let close_res = vfs.close(fh).await.map_err(io::Error::from);
    match (data_res, close_res) {
        (Ok(data), Ok(())) => Ok(data),
        (Err(err), _) => Err(err),
        (Ok(_), Err(err)) => Err(err),
    }
}

async fn verify_path(vfs: &SlayerVfs, model_root: &Path, name: &str) {
    let sp = fs_path(name);
    let mp = model_path(model_root, name);

    let slayer_meta = vfs.stat(&sp).await.map_err(io::Error::from);
    let model_meta = fs::symlink_metadata(&mp);

    match (slayer_meta, model_meta) {
        (Ok(sm), Ok(mm)) => {
            compare_attrs("verify", &sp, &sm, &mm);

            if sm.kind == VfsFileType::File {
                match fs::read(&mp) {
                    Ok(expect) => {
                        let got = slayer_read_file(vfs, &sm, expect.len())
                            .await
                            .expect("slayerfs read failed");
                        assert_eq!(got, expect, "verify: content mismatch at {sp}");
                    }
                    Err(err) if err.kind() == io::ErrorKind::PermissionDenied => {
                        // Chmod may temporarily make model files unreadable for this process.
                        // In that case we still validate metadata shape and skip byte comparison.
                    }
                    Err(err) => panic!("model read failed: {err:?}"),
                }
            } else if sm.kind == VfsFileType::Symlink {
                let got_target = vfs.readlink(&sp).await.expect("slayerfs readlink failed");
                let expect_target = fs::read_link(&mp)
                    .expect("model readlink failed")
                    .to_string_lossy()
                    .into_owned();
                assert_eq!(
                    got_target, expect_target,
                    "verify: symlink target mismatch at {sp}"
                );
            }
        }
        (Err(_), Err(_)) => {}
        (Ok(_), Err(e)) => panic!("verify: existence mismatch, slayerfs has {sp}, model err={e:?}"),
        (Err(e), Ok(_)) => panic!("verify: existence mismatch, model has {sp}, slayerfs err={e:?}"),
    }
}

async fn verify_all(vfs: &SlayerVfs, model_root: &Path) {
    for dir in DIR_NAMES {
        verify_path(vfs, model_root, dir).await;
        for name in ALL_NAMES {
            let rel = format!("{dir}/{name}");
            verify_path(vfs, model_root, &rel).await;
        }
    }
}

fn validate_open_spec(
    mode: AccessMode,
    create: bool,
    excl: bool,
    trunc: bool,
    append: bool,
    directory: bool,
) -> io::Result<()> {
    let create = create || excl;
    let write = mode.write();

    if append && trunc {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "append and truncate cannot be set together",
        ));
    }
    if (create || trunc || append) && !write {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "create/truncate/append requires writable open",
        ));
    }
    if directory && (write || create || trunc || append || excl) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "directory open does not support write/create/truncate flags",
        ));
    }
    Ok(())
}

#[derive(Debug)]
struct SlayerFileFd {
    fh: u64,
    path: String,
    ino: i64,
    offset: u64,
    read: bool,
    write: bool,
    append: bool,
}

#[derive(Debug)]
struct SlayerDirFd {
    fh: u64,
    path: String,
    offset: u64,
}

#[derive(Debug)]
enum SlayerFd {
    File(SlayerFileFd),
    Dir(SlayerDirFd),
}

impl SlayerFd {
    fn path(&self) -> &str {
        match self {
            SlayerFd::File(fd) => &fd.path,
            SlayerFd::Dir(fd) => &fd.path,
        }
    }
}

struct ModelFileFd {
    file: File,
    path: PathBuf,
    offset: u64,
    read: bool,
    write: bool,
    append: bool,
}

struct ModelDirFd {
    path: PathBuf,
    offset: u64,
    // Snapshot taken at open-time to mirror SlayerFS DirHandle semantics.
    entries: Vec<String>,
}

enum ModelFd {
    File(ModelFileFd),
    Dir(ModelDirFd),
}

impl ModelFd {
    fn path(&self) -> &Path {
        match self {
            ModelFd::File(fd) => &fd.path,
            ModelFd::Dir(fd) => &fd.path,
        }
    }
}

struct FdTables {
    slayer: Vec<Option<SlayerFd>>,
    model: Vec<Option<ModelFd>>,
    slot_owner: Vec<Option<usize>>,
}

impl FdTables {
    fn new() -> Self {
        let slayer = std::iter::repeat_with(|| None).take(MAX_FDS).collect();
        let model = std::iter::repeat_with(|| None).take(MAX_FDS).collect();
        let slot_owner = std::iter::repeat_with(|| None).take(MAX_FDS).collect();
        Self {
            slayer,
            model,
            slot_owner,
        }
    }

    fn has_file_fd(&self) -> bool {
        self.slayer
            .iter()
            .any(|fd| matches!(fd, Some(SlayerFd::File(_))))
    }

    fn has_dir_fd(&self) -> bool {
        self.slayer
            .iter()
            .any(|fd| matches!(fd, Some(SlayerFd::Dir(_))))
    }

    fn assert_shape(&self, op: &str) {
        for idx in 0..MAX_FDS {
            let slayer_has = self.slayer[idx].is_some();
            let model_has = self.model[idx].is_some();
            let owner_has = self.slot_owner[idx].is_some();
            assert_eq!(
                slayer_has, model_has,
                "{op}: fd table shape mismatch at slot {idx}"
            );
            assert_eq!(
                slayer_has, owner_has,
                "{op}: fd owner mismatch at slot {idx}"
            );
        }
    }
}

async fn close_slayer_slot(vfs: &SlayerVfs, slot: &mut Option<SlayerFd>) -> io::Result<()> {
    match slot.take() {
        Some(SlayerFd::File(fd)) => vfs.close(fd.fh).await.map_err(io::Error::from),
        Some(SlayerFd::Dir(fd)) => vfs.closedir(fd.fh).map_err(io::Error::from),
        None => Err(bad_fd()),
    }
}

fn close_model_slot(slot: &mut Option<ModelFd>) -> io::Result<()> {
    match slot.take() {
        Some(_) => Ok(()),
        None => Err(bad_fd()),
    }
}

fn slot_owner_or(fds: &FdTables, slot: usize, fallback: usize) -> usize {
    fds.slot_owner[slot].unwrap_or(fallback)
}

fn slayer_path_open_offset_hint(fds: &FdTables, path: &str) -> Option<u64> {
    fds.slayer
        .iter()
        .filter_map(|slot| match slot {
            Some(SlayerFd::File(fd)) if fd.path == path => Some(fd.offset),
            _ => None,
        })
        .max()
}

fn model_path_open_offset_hint(fds: &FdTables, path: &Path) -> Option<u64> {
    fds.model
        .iter()
        .filter_map(|slot| match slot {
            Some(ModelFd::File(fd)) if fd.path == path => Some(fd.offset),
            _ => None,
        })
        .max()
}

fn remap_slayer_path(path: &str, old_path: &str, new_path: &str) -> Option<String> {
    if path == old_path {
        return Some(new_path.to_string());
    }

    let suffix = path.strip_prefix(old_path)?;
    if !suffix.starts_with('/') {
        return None;
    }

    Some(format!("{new_path}{suffix}"))
}

fn remap_model_path(path: &Path, old_path: &Path, new_path: &Path) -> Option<PathBuf> {
    if path == old_path {
        return Some(new_path.to_path_buf());
    }

    let suffix = path.strip_prefix(old_path).ok()?;
    Some(new_path.join(suffix))
}

fn refresh_fd_paths_after_rename(
    fds: &mut FdTables,
    old_sp: &str,
    new_sp: &str,
    old_mp: &Path,
    new_mp: &Path,
) {
    for slot in 0..MAX_FDS {
        if let Some(sfd) = fds.slayer[slot].as_mut() {
            match sfd {
                SlayerFd::File(fd) => {
                    if let Some(new_path) = remap_slayer_path(&fd.path, old_sp, new_sp) {
                        fd.path = new_path;
                    }
                }
                SlayerFd::Dir(fd) => {
                    if let Some(new_path) = remap_slayer_path(&fd.path, old_sp, new_sp) {
                        fd.path = new_path;
                    }
                }
            }
        }

        if let Some(mfd) = fds.model[slot].as_mut() {
            match mfd {
                ModelFd::File(fd) => {
                    if let Some(new_path) = remap_model_path(&fd.path, old_mp, new_mp) {
                        fd.path = new_path;
                    }
                }
                ModelFd::Dir(fd) => {
                    if let Some(new_path) = remap_model_path(&fd.path, old_mp, new_mp) {
                        fd.path = new_path;
                    }
                }
            }
        }
    }
}

async fn open_slayer_fd(
    vfs: &SlayerVfs,
    path: &str,
    mode: AccessMode,
    create: bool,
    excl: bool,
    trunc: bool,
    append: bool,
    nofollow: bool,
    directory: bool,
) -> io::Result<SlayerFd> {
    validate_open_spec(mode, create, excl, trunc, append, directory)?;

    let create = create || excl;
    if create {
        vfs.create_file_in_existing_dir_err(path, excl)
            .await
            .map_err(io::Error::from)?;
    }

    let mut attr = vfs.stat(path).await.map_err(io::Error::from)?;

    if nofollow && attr.kind == VfsFileType::Symlink {
        return Err(loop_error());
    }

    if directory {
        if attr.kind != VfsFileType::Dir {
            return Err(io::Error::new(
                io::ErrorKind::NotADirectory,
                path.to_string(),
            ));
        }
        let fh = vfs.opendir(attr.ino).await.map_err(io::Error::from)?;
        return Ok(SlayerFd::Dir(SlayerDirFd {
            fh,
            path: path.to_string(),
            offset: 0,
        }));
    }

    if attr.kind == VfsFileType::Dir {
        return Err(io::Error::new(
            io::ErrorKind::IsADirectory,
            path.to_string(),
        ));
    }
    if attr.kind != VfsFileType::File {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "unsupported file kind",
        ));
    }

    if trunc && mode.write() {
        vfs.truncate_inode(attr.ino, 0)
            .await
            .map_err(io::Error::from)?;
        attr = vfs.stat(path).await.map_err(io::Error::from)?;
    }

    let fh = vfs
        .open(attr.ino, attr.clone(), mode.read(), mode.write())
        .await
        .map_err(io::Error::from)?;
    let offset = if append { attr.size } else { 0 };

    Ok(SlayerFd::File(SlayerFileFd {
        fh,
        path: path.to_string(),
        ino: attr.ino,
        offset,
        read: mode.read(),
        write: mode.write(),
        append,
    }))
}

fn open_model_fd(
    path: &Path,
    mode: AccessMode,
    create: bool,
    excl: bool,
    trunc: bool,
    append: bool,
    nofollow: bool,
    directory: bool,
) -> io::Result<ModelFd> {
    validate_open_spec(mode, create, excl, trunc, append, directory)?;

    if nofollow
        && let Ok(meta) = fs::symlink_metadata(path)
        && meta.file_type().is_symlink()
    {
        return Err(loop_error());
    }

    if directory {
        let meta = fs::symlink_metadata(path)?;
        if !meta.file_type().is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::NotADirectory,
                path.display().to_string(),
            ));
        }
        let mut entries = Vec::new();
        for ent in fs::read_dir(path)? {
            let ent = ent?;
            entries.push(ent.file_name().to_string_lossy().into_owned());
        }
        entries.sort_unstable();

        return Ok(ModelFd::Dir(ModelDirFd {
            path: path.to_path_buf(),
            offset: 0,
            entries,
        }));
    }

    let create = create || excl;
    let mut opts = OpenOptions::new();
    opts.read(mode.read()).write(mode.write());

    if excl {
        opts.create_new(true);
    } else if create {
        opts.create(true);
    }
    if trunc {
        opts.truncate(true);
    }

    let file = opts.open(path)?;
    let offset = if append { file.metadata()?.len() } else { 0 };

    Ok(ModelFd::File(ModelFileFd {
        file,
        path: path.to_path_buf(),
        offset,
        read: mode.read(),
        write: mode.write(),
        append,
    }))
}

fn compute_seek(current: u64, end: u64, whence: SeekWhence, delta: i64) -> io::Result<u64> {
    let base: i128 = match whence {
        SeekWhence::Start => 0,
        SeekWhence::Current => current as i128,
        SeekWhence::End => end as i128,
    };

    let next = base
        .checked_add(delta as i128)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek overflow"))?;

    if next < 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid seek to a negative position",
        ));
    }

    Ok(next as u64)
}

fn model_readdir_chunk(entries: &[String], offset: u64) -> Vec<String> {
    let start = offset as usize;
    if start >= entries.len() {
        return Vec::new();
    }
    let end = entries.len().min(start + MAX_READDIR_ENTRIES);
    entries[start..end].to_vec()
}

fn model_mkdir_like_slayer(path: &Path) -> io::Result<()> {
    match fs::create_dir(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
            match fs::symlink_metadata(path) {
                Ok(meta) if meta.file_type().is_dir() => Ok(()),
                _ => Err(err),
            }
        }
        Err(err) => Err(err),
    }
}

fn subtree_unlink_targets(dir: &str) -> Vec<String> {
    let mut out = Vec::new();

    for sub in SUBDIR_NAMES {
        for nested in SUBDIR_NAMES {
            for name in UNLINK_NAMES {
                out.push(format!("{dir}/{sub}/{nested}/{name}"));
            }
        }
        for name in UNLINK_NAMES {
            out.push(format!("{dir}/{sub}/{name}"));
        }
    }

    for name in UNLINK_NAMES {
        out.push(format!("{dir}/{name}"));
    }

    out
}

fn subtree_rmdir_targets(dir: &str) -> Vec<String> {
    let mut out = Vec::new();

    for sub in SUBDIR_NAMES {
        for nested in SUBDIR_NAMES {
            out.push(format!("{dir}/{sub}/{nested}"));
        }
    }
    for sub in SUBDIR_NAMES {
        out.push(format!("{dir}/{sub}"));
    }

    out
}

fn model_utimens(
    path: &Path,
    atime_ns: i64,
    mtime_ns: i64,
    atime_now: bool,
    mtime_now: bool,
) -> io::Result<()> {
    fn to_timespec(nanos: i64) -> libc::timespec {
        let secs = nanos.div_euclid(1_000_000_000);
        let nsec = nanos.rem_euclid(1_000_000_000);
        libc::timespec {
            tv_sec: secs as libc::time_t,
            tv_nsec: nsec as libc::c_long,
        }
    }

    let mut ts = [to_timespec(atime_ns), to_timespec(mtime_ns)];
    if atime_now {
        ts[0].tv_nsec = libc::UTIME_NOW as libc::c_long;
    }
    if mtime_now {
        ts[1].tv_nsec = libc::UTIME_NOW as libc::c_long;
    }

    let c_path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains NUL"))?;

    // SAFETY: `c_path` and `ts` are valid pointers for the duration of this call.
    let rc = unsafe {
        libc::utimensat(
            libc::AT_FDCWD,
            c_path.as_ptr(),
            ts.as_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };

    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[derive(Debug, Clone)]
enum FuzzMetaConfig {
    Sqlite { url: String },
    Etcd { urls: Vec<String> },
}

async fn new_local_vfs(root: &Path, meta: &FuzzMetaConfig) -> io::Result<SlayerVfs> {
    let backend = ObjectClient::new(LocalFsBackend::new(root));
    let store = ObjectBlockStore::new(backend);

    let client = ClientOptions {
        no_background_jobs: true,
        ..ClientOptions::default()
    };

    let meta_store: Arc<dyn MetaStore> = match meta {
        FuzzMetaConfig::Sqlite { url } => {
            let cfg = Config {
                database: DatabaseConfig {
                    db_config: DatabaseType::Sqlite { url: url.clone() },
                },
                cache: CacheConfig::default(),
                client,
            };
            let store = DatabaseMetaStore::from_config(cfg)
                .await
                .map_err(io::Error::other)?;
            Arc::new(store)
        }
        FuzzMetaConfig::Etcd { urls } => {
            let cfg = Config {
                database: DatabaseConfig {
                    db_config: DatabaseType::Etcd { urls: urls.clone() },
                },
                cache: CacheConfig::default(),
                client,
            };
            let store = EtcdMetaStore::from_config(cfg)
                .await
                .map_err(io::Error::other)?;
            Arc::new(store)
        }
    };

    VFS::new(LAYOUT, store, meta_store)
        .await
        .map_err(io::Error::from)
}

fn fuzz_clients_from_env() -> usize {
    std::env::var("SLAYERFS_FUZZ_CLIENTS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .map(|v| v.clamp(1, 32))
        .unwrap_or(1)
}

fn fuzz_meta_url_from_env() -> String {
    std::env::var("SLAYERFS_FUZZ_META_URL")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "sqlite::memory:".to_string())
}

fn fuzz_etcd_urls_from_env() -> Vec<String> {
    std::env::var("SLAYERFS_FUZZ_ETCD_URLS")
        .or_else(|_| std::env::var("SLAYERFS_FUZZ_META_ETCD_URLS"))
        .ok()
        .map(|raw| {
            raw.split(',')
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .collect::<Vec<_>>()
        })
        .filter(|urls| !urls.is_empty())
        .unwrap_or_else(|| vec!["127.0.0.1:2379".to_string()])
}

fn fuzz_meta_config_from_env() -> FuzzMetaConfig {
    let backend = std::env::var("SLAYERFS_FUZZ_META_BACKEND")
        .ok()
        .unwrap_or_else(|| "sqlite".to_string());

    if backend.eq_ignore_ascii_case("etcd") {
        FuzzMetaConfig::Etcd {
            urls: fuzz_etcd_urls_from_env(),
        }
    } else {
        FuzzMetaConfig::Sqlite {
            url: fuzz_meta_url_from_env(),
        }
    }
}

async fn reset_etcd_for_case(urls: &[String]) -> io::Result<()> {
    let mut client = RawEtcdClient::connect(urls.to_vec(), None)
        .await
        .map_err(io::Error::other)?;
    client
        .delete("", Some(DeleteOptions::new().with_prefix()))
        .await
        .map_err(io::Error::other)?;
    Ok(())
}

fn fuzz_concurrent_from_env() -> bool {
    std::env::var("SLAYERFS_FUZZ_CONCURRENT")
        .ok()
        .map(|v| {
            let v = v.trim();
            v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("yes")
        })
        .unwrap_or(false)
}

fn fuzz_concurrent_batch_from_env() -> usize {
    std::env::var("SLAYERFS_FUZZ_CONCURRENT_BATCH")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .map(|v| v.clamp(1, 64))
        .unwrap_or(8)
}

async fn run_multiple_clients_case(data: &[u8], clients: usize, meta: &FuzzMetaConfig) {
    run_clients_case(data, clients.max(1), meta).await;
}

async fn run_case(data: &[u8]) {
    let clients = fuzz_clients_from_env();
    ACTIVE_FUZZ_CLIENTS.store(clients, Ordering::Relaxed);

    let meta = fuzz_meta_config_from_env();

    if let FuzzMetaConfig::Etcd { urls } = &meta
        && reset_etcd_for_case(urls).await.is_err()
    {
        return;
    }

    if fuzz_concurrent_from_env() {
        run_clients_case_concurrent(data, clients, &meta).await;
    } else {
        run_multiple_clients_case(data, clients, &meta).await;
    }
}

fn meta_config_for_clients(meta: &FuzzMetaConfig, clients: usize, root: &Path) -> FuzzMetaConfig {
    match meta {
        FuzzMetaConfig::Sqlite { url } => {
            if clients > 1 && url.trim() == "sqlite::memory:" {
                let db_path = root.join("fuzz-meta.db");
                FuzzMetaConfig::Sqlite {
                    url: format!("sqlite://{}?mode=rwc", db_path.display()),
                }
            } else {
                FuzzMetaConfig::Sqlite { url: url.clone() }
            }
        }
        FuzzMetaConfig::Etcd { urls } => FuzzMetaConfig::Etcd { urls: urls.clone() },
    }
}

fn op_slot(op: &Op) -> Option<usize> {
    match op {
        Op::Open { slot, .. }
        | Op::Close { slot }
        | Op::Lseek { slot, .. }
        | Op::ReadFd { slot, .. }
        | Op::WriteFd { slot, .. }
        | Op::PRead { slot, .. }
        | Op::PWrite { slot, .. }
        | Op::Readdir { slot }
        | Op::Fstat { slot }
        | Op::Ftruncate { slot, .. }
        | Op::Fsync { slot }
        | Op::Fdatasync { slot }
        | Op::Fchmod { slot, .. } => Some(*slot),
        _ => None,
    }
}

fn use_observer_for_op(op: &Op) -> bool {
    matches!(
        op,
        Op::Create { .. }
            | Op::Unlink { .. }
            | Op::Mkdir { .. }
            | Op::Rmdir { .. }
            | Op::Readlink { .. }
            | Op::Stat { .. }
            | Op::Lstat { .. }
            | Op::Utimens { .. }
            | Op::Symlink { .. }
            | Op::Link { .. }
            | Op::Rename { .. }
            | Op::PurgeDir { .. }
            | Op::Chmod { .. }
    )
}

async fn verify_all_clients(vfs_clients: &[SlayerVfs], model_root: &Path) {
    if let Some(vfs) = vfs_clients.first() {
        verify_all(vfs, model_root).await;
    }
}

async fn close_all_slots(vfs_clients: &[SlayerVfs], fds: &mut FdTables, op: &str) {
    for slot in 0..MAX_FDS {
        if fds.slayer[slot].is_none() && fds.model[slot].is_none() {
            continue;
        }

        fds.assert_shape(op);

        let owner = slot_owner_or(fds, slot, 0);
        let slayer_res = close_slayer_slot(&vfs_clients[owner], &mut fds.slayer[slot]).await;
        let model_res = close_model_slot(&mut fds.model[slot]);

        let _ = compare_outcome(op, slayer_res, model_res);
        fds.slot_owner[slot] = None;
    }
}

async fn verify_concurrent_sanity(vfs: &SlayerVfs) {
    for dir in DIR_NAMES {
        let sp = fs_path(dir);
        let attr = match vfs.stat(&sp).await {
            Ok(attr) => attr,
            Err(_) => continue,
        };

        assert_eq!(
            attr.kind,
            VfsFileType::Dir,
            "concurrent-verify: expected directory at {sp}"
        );

        let fh = vfs
            .opendir(attr.ino)
            .await
            .expect("concurrent-verify: opendir failed");

        let mut offset = 0;
        let mut names = Vec::new();
        for _ in 0..16 {
            let batch = vfs
                .readdir(fh, offset)
                .unwrap_or_else(|| panic!("concurrent-verify: readdir failed for {sp}"));
            if batch.is_empty() {
                break;
            }
            offset += batch.len() as u64;
            for entry in batch {
                if entry.name != "." && entry.name != ".." {
                    names.push(entry.name);
                }
            }
        }

        vfs.closedir(fh)
            .unwrap_or_else(|err| panic!("concurrent-verify: closedir failed for {sp}: {err}"));

        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            names.len(),
            sorted.len(),
            "concurrent-verify: duplicate directory entries detected at {sp}"
        );
    }

    for dir in DIR_NAMES {
        for name in ALL_NAMES {
            let rel = format!("{dir}/{name}");
            let sp = fs_path(&rel);
            let attr = match vfs.stat(&sp).await {
                Ok(attr) => attr,
                Err(_) => continue,
            };

            if attr.kind == VfsFileType::Symlink {
                let _ = vfs.readlink(&sp).await.unwrap_or_else(|err| {
                    panic!("concurrent-verify: readlink failed for {sp}: {err}")
                });
            }
        }
    }
}

async fn apply_concurrent_op(vfs: &SlayerVfs, op: Op) {
    match op {
        Op::Open {
            path,
            mode,
            create,
            excl,
            trunc,
            append,
            nofollow,
            directory,
            ..
        } => {
            let sp = fs_path(&path);
            if let Ok(fd) = open_slayer_fd(
                vfs, &sp, mode, create, excl, trunc, append, nofollow, directory,
            )
            .await
            {
                match fd {
                    SlayerFd::File(fd) => {
                        let _ = vfs.close(fd.fh).await;
                    }
                    SlayerFd::Dir(fd) => {
                        let _ = vfs.closedir(fd.fh);
                    }
                }
            }
        }
        Op::Create { path, exclusive } => {
            let _ = vfs
                .create_file_in_existing_dir_err(&fs_path(&path), exclusive)
                .await;
        }
        Op::Unlink { path } => {
            let _ = vfs.unlink(&fs_path(&path)).await;
        }
        Op::Mkdir { path } => {
            let _ = vfs.mkdir_err(&fs_path(&path)).await;
        }
        Op::Rmdir { path } => {
            let _ = vfs.rmdir(&fs_path(&path)).await;
        }
        Op::Readlink { path } => {
            let _ = vfs.readlink(&fs_path(&path)).await;
        }
        Op::Stat { path } | Op::Lstat { path } => {
            let _ = vfs.stat(&fs_path(&path)).await;
        }
        Op::Utimens {
            path,
            atime_ns,
            mtime_ns,
            atime_now,
            mtime_now,
        } => {
            let sp = fs_path(&path);
            if let Ok(attr) = vfs.stat(&sp).await {
                let mut req = SetAttrRequest::default();
                let mut flags = SetAttrFlags::empty();

                if atime_now {
                    flags |= SetAttrFlags::SET_ATIME_NOW;
                } else {
                    req.atime = Some(atime_ns);
                }

                if mtime_now {
                    flags |= SetAttrFlags::SET_MTIME_NOW;
                } else {
                    req.mtime = Some(mtime_ns);
                }

                let _ = vfs.set_attr(attr.ino, &req, flags).await;
            }
        }
        Op::Symlink { src, dst } => {
            let _ = vfs.create_symlink(&fs_path(&dst), &fs_path(&src)).await;
        }
        Op::Link { src, dst } => {
            let _ = vfs.link(&fs_path(&src), &fs_path(&dst)).await;
        }
        Op::Rename { src, dst } => {
            let _ = vfs.rename(&fs_path(&src), &fs_path(&dst)).await;
        }
        Op::PurgeDir { dir, recreate } => {
            let root_sp = fs_path(&dir);

            for rel in subtree_unlink_targets(&dir) {
                let _ = vfs.unlink(&fs_path(&rel)).await;
            }
            for rel in subtree_rmdir_targets(&dir) {
                let _ = vfs.rmdir(&fs_path(&rel)).await;
            }
            let _ = vfs.rmdir(&root_sp).await;
            if recreate {
                let _ = vfs.mkdir_p(&root_sp).await;
            }
        }
        Op::Chmod { path, mode } => {
            let sp = fs_path(&path);
            if let Ok(attr) = vfs.stat(&sp).await {
                let req = SetAttrRequest {
                    mode: Some(mode),
                    ..Default::default()
                };
                let _ = vfs.set_attr(attr.ino, &req, SetAttrFlags::empty()).await;
            }
        }
        Op::Close { .. }
        | Op::Lseek { .. }
        | Op::ReadFd { .. }
        | Op::WriteFd { .. }
        | Op::PRead { .. }
        | Op::PWrite { .. }
        | Op::Readdir { .. }
        | Op::Fstat { .. }
        | Op::Ftruncate { .. }
        | Op::Fsync { .. }
        | Op::Fdatasync { .. }
        | Op::Fchmod { .. } => {}
    }
}

async fn run_clients_case_concurrent(data: &[u8], clients: usize, meta: &FuzzMetaConfig) {
    let mut cur = ByteCursor::new(data);
    let op_count = match cur.next_u8() {
        Some(v) => 8 + (v as usize % 160),
        None => return,
    };

    let slayer_store = match tempfile::tempdir() {
        Ok(d) => d,
        Err(_) => return,
    };

    let clients = clients.max(1);
    let resolved_meta = meta_config_for_clients(meta, clients, slayer_store.path());
    let mut vfs_clients = Vec::with_capacity(clients);
    for _ in 0..clients {
        let vfs = match new_local_vfs(slayer_store.path(), &resolved_meta).await {
            Ok(v) => v,
            Err(_) => return,
        };
        vfs_clients.push(vfs);
    }

    let primary_vfs = &vfs_clients[0];
    if primary_vfs.mkdir_p("/fuzz").await.is_err() {
        return;
    }
    for dir in DIR_NAMES {
        if primary_vfs.mkdir_p(&fs_path(dir)).await.is_err() {
            return;
        }
    }

    let mut scheduled = Vec::with_capacity(op_count);
    for _ in 0..op_count {
        // Concurrent mode intentionally avoids fd lifecycle coupling, so decode without fd bias.
        let op = match cur.next_op(true, true) {
            Some(v) => v,
            None => break,
        };

        let client_idx = if clients > 1 {
            cur.next_u8().map(|v| v as usize % clients).unwrap_or(0)
        } else {
            0
        };
        scheduled.push((client_idx, op));
    }

    let batch_size = fuzz_concurrent_batch_from_env();
    for chunk in scheduled.chunks(batch_size) {
        let mut join_set = JoinSet::new();
        for (client_idx, op) in chunk {
            let vfs = vfs_clients[*client_idx].clone();
            let op = op.clone();
            join_set.spawn(async move {
                apply_concurrent_op(&vfs, op).await;
            });
        }

        while let Some(res) = join_set.join_next().await {
            if let Err(err) = res {
                panic!("concurrent op task panicked: {err}");
            }
        }

        if let Ok(observer) = new_local_vfs(slayer_store.path(), &resolved_meta).await {
            verify_concurrent_sanity(&observer).await;
        }
    }

    if let Ok(observer) = new_local_vfs(slayer_store.path(), &resolved_meta).await {
        verify_concurrent_sanity(&observer).await;
    }
}

async fn run_clients_case(data: &[u8], clients: usize, meta: &FuzzMetaConfig) {
    let mut cur = ByteCursor::new(data);
    let op_count = match cur.next_u8() {
        Some(v) => 8 + (v as usize % 160),
        None => return,
    };

    let slayer_store = match tempfile::tempdir() {
        Ok(d) => d,
        Err(_) => return,
    };
    let model_root = match tempfile::tempdir() {
        Ok(d) => d,
        Err(_) => return,
    };

    let clients = clients.max(1);
    let resolved_meta = meta_config_for_clients(meta, clients, slayer_store.path());
    let mut vfs_clients = Vec::with_capacity(clients);
    for _ in 0..clients {
        let vfs = match new_local_vfs(slayer_store.path(), &resolved_meta).await {
            Ok(v) => v,
            Err(_) => return,
        };
        vfs_clients.push(vfs);
    }
    let primary_vfs = &vfs_clients[0];

    if primary_vfs.mkdir_p("/fuzz").await.is_err() {
        return;
    }
    if fs::create_dir_all(model_root.path().join("fuzz")).is_err() {
        return;
    }

    for dir in DIR_NAMES {
        let sp = fs_path(dir);
        let mp = model_path(model_root.path(), dir);
        if primary_vfs.mkdir_p(&sp).await.is_err() {
            return;
        }
        if fs::create_dir_all(mp).is_err() {
            return;
        }
    }

    let mut fds = FdTables::new();
    if clients == 1 {
        verify_all_clients(&vfs_clients, model_root.path()).await;
    }

    for i in 0..op_count {
        fds.assert_shape("loop-start");

        let op = match cur.next_op(fds.has_file_fd(), fds.has_dir_fd()) {
            Some(v) => v,
            None => break,
        };

        let sampled_client = if clients > 1 {
            cur.next_u8().map(|v| v as usize % clients).unwrap_or(0)
        } else {
            0
        };

        let mut client_idx = sampled_client;
        if !matches!(op, Op::Open { .. })
            && let Some(slot) = op_slot(&op)
            && fds.slayer[slot].is_some()
        {
            client_idx = slot_owner_or(&fds, slot, sampled_client);
        }

        let mut observer_vfs = None;
        if clients > 1 && use_observer_for_op(&op) {
            observer_vfs = new_local_vfs(slayer_store.path(), &resolved_meta)
                .await
                .ok();
        }

        let vfs = observer_vfs.as_ref().unwrap_or(&vfs_clients[client_idx]);

        match op {
            Op::Open {
                slot,
                path,
                mode,
                create,
                excl,
                trunc,
                append,
                nofollow,
                directory,
            } => {
                let sp = fs_path(&path);
                let mp = model_path(model_root.path(), &path);

                let slayer_res = open_slayer_fd(
                    &vfs, &sp, mode, create, excl, trunc, append, nofollow, directory,
                )
                .await;
                let model_res =
                    open_model_fd(&mp, mode, create, excl, trunc, append, nofollow, directory);

                if clients > 1
                    && let (Err(err), Ok(_)) = (&slayer_res, &model_res)
                    && is_notfound_like(err)
                {
                    // Multi-client mode can observe stale per-client path caches.
                    // Verify with a fresh observer client before deciding this is a bug.
                    if let Ok(observer) = new_local_vfs(slayer_store.path(), &resolved_meta).await {
                        let retry_res = open_slayer_fd(
                            &observer, &sp, mode, create, excl, trunc, append, nofollow, directory,
                        )
                        .await;
                        if retry_res.is_ok() {
                            continue;
                        }
                    }
                }

                if clients > 1 && slayer_res.is_err() != model_res.is_err() {
                    if let Ok(fd) = slayer_res {
                        match fd {
                            SlayerFd::File(fd) => {
                                let _ = vfs.close(fd.fh).await;
                            }
                            SlayerFd::Dir(fd) => {
                                let _ = vfs.closedir(fd.fh);
                            }
                        }
                    }
                    continue;
                }

                if let Some((mut new_slayer, mut new_model)) =
                    compare_outcome("open", slayer_res, model_res)
                {
                    if let Some(hint) = slayer_path_open_offset_hint(&fds, &sp)
                        && let SlayerFd::File(fd) = &mut new_slayer
                        && fd.append
                    {
                        fd.offset = fd.offset.max(hint);
                    }
                    if let Some(hint) = model_path_open_offset_hint(&fds, &mp)
                        && let ModelFd::File(fd) = &mut new_model
                        && fd.append
                    {
                        fd.offset = fd.offset.max(hint);
                    }

                    if fds.slayer[slot].is_some() || fds.model[slot].is_some() {
                        fds.assert_shape("open-replace");
                        let owner = slot_owner_or(&fds, slot, client_idx);
                        let old_slayer =
                            close_slayer_slot(&vfs_clients[owner], &mut fds.slayer[slot]).await;
                        let old_model = close_model_slot(&mut fds.model[slot]);
                        let _ = compare_outcome("open-replace-close", old_slayer, old_model);
                        fds.slot_owner[slot] = None;
                    }

                    fds.slayer[slot] = Some(new_slayer);
                    fds.model[slot] = Some(new_model);
                    fds.slot_owner[slot] = Some(client_idx);
                }
            }
            Op::Close { slot } => {
                let owner = slot_owner_or(&fds, slot, client_idx);
                let slayer_res =
                    close_slayer_slot(&vfs_clients[owner], &mut fds.slayer[slot]).await;
                let model_res = close_model_slot(&mut fds.model[slot]);
                let _ = compare_outcome("close", slayer_res, model_res);
                fds.slot_owner[slot] = None;
            }
            Op::Lseek {
                slot,
                whence,
                delta,
            } => {
                let slayer_res: io::Result<u64> = async {
                    let (path, cur_off) = match fds.slayer[slot].as_ref() {
                        Some(SlayerFd::File(fd)) => (fd.path.clone(), fd.offset),
                        _ => return Err(bad_fd()),
                    };
                    let end = vfs.stat(&path).await.map_err(io::Error::from)?.size;
                    let next = compute_seek(cur_off, end, whence, delta)?;
                    if let Some(SlayerFd::File(fd)) = fds.slayer[slot].as_mut() {
                        fd.offset = next;
                    }
                    Ok(next)
                }
                .await;

                let model_res: io::Result<u64> = (|| {
                    let fd = match fds.model[slot].as_mut() {
                        Some(ModelFd::File(fd)) => fd,
                        _ => return Err(bad_fd()),
                    };
                    let end = fs::metadata(&fd.path)?.len();
                    let next = compute_seek(fd.offset, end, whence, delta)?;
                    fd.offset = next;
                    Ok(next)
                })();

                if let Some((got, expect)) = compare_outcome("lseek", slayer_res, model_res) {
                    assert_eq!(got, expect, "lseek: offset mismatch at slot {slot}");
                }
            }
            Op::ReadFd { slot, len } => {
                let slayer_res: io::Result<Vec<u8>> = async {
                    let (fh, cur_off, can_read) = match fds.slayer[slot].as_ref() {
                        Some(SlayerFd::File(fd)) => (fd.fh, fd.offset, fd.read),
                        _ => return Err(bad_fd()),
                    };

                    if !can_read {
                        return Err(io::Error::new(
                            io::ErrorKind::PermissionDenied,
                            "fd not opened for read",
                        ));
                    }

                    let data = vfs.read(fh, cur_off, len).await.map_err(io::Error::from)?;
                    if let Some(SlayerFd::File(fd)) = fds.slayer[slot].as_mut() {
                        fd.offset = cur_off + data.len() as u64;
                    }
                    Ok(data)
                }
                .await;

                let model_res: io::Result<Vec<u8>> = (|| {
                    let fd = match fds.model[slot].as_mut() {
                        Some(ModelFd::File(fd)) => fd,
                        _ => return Err(bad_fd()),
                    };
                    if !fd.read {
                        return Err(io::Error::new(
                            io::ErrorKind::PermissionDenied,
                            "fd not opened for read",
                        ));
                    }

                    fd.file.seek(SeekFrom::Start(fd.offset))?;
                    let mut buf = vec![0u8; len];
                    let n = fd.file.read(&mut buf)?;
                    buf.truncate(n);
                    fd.offset += n as u64;
                    Ok(buf)
                })();

                if let Some((got, expect)) = compare_outcome("readfd", slayer_res, model_res) {
                    assert_eq!(got, expect, "readfd: data mismatch at slot {slot}");
                }
            }
            Op::WriteFd { slot, data } => {
                let slayer_res: io::Result<usize> = async {
                    let (fh, cur_off, can_write) = match fds.slayer[slot].as_ref() {
                        Some(SlayerFd::File(fd)) => (fd.fh, fd.offset, fd.write),
                        _ => return Err(bad_fd()),
                    };

                    if !can_write {
                        return Err(io::Error::new(
                            io::ErrorKind::PermissionDenied,
                            "fd not opened for write",
                        ));
                    }

                    let write_offset = cur_off;
                    let n = vfs
                        .write(fh, write_offset, &data)
                        .await
                        .map_err(io::Error::from)?;

                    if let Some(SlayerFd::File(fd)) = fds.slayer[slot].as_mut() {
                        fd.offset = write_offset + n as u64;
                    }
                    Ok(n)
                }
                .await;

                let model_res: io::Result<usize> = (|| {
                    let fd = match fds.model[slot].as_mut() {
                        Some(ModelFd::File(fd)) => fd,
                        _ => return Err(bad_fd()),
                    };
                    if !fd.write {
                        return Err(io::Error::new(
                            io::ErrorKind::PermissionDenied,
                            "fd not opened for write",
                        ));
                    }

                    let write_offset = fd.offset;
                    fd.file.seek(SeekFrom::Start(write_offset))?;
                    fd.file.write_all(&data)?;
                    fd.offset = write_offset + data.len() as u64;
                    Ok(data.len())
                })();

                if let Some((got, expect)) = compare_outcome("writefd", slayer_res, model_res) {
                    assert_eq!(got, expect, "writefd: byte count mismatch at slot {slot}");
                }
            }
            Op::PRead { slot, off, len } => {
                let slayer_res: io::Result<Vec<u8>> = async {
                    let (fh, can_read) = match fds.slayer[slot].as_ref() {
                        Some(SlayerFd::File(fd)) => (fd.fh, fd.read),
                        _ => return Err(bad_fd()),
                    };

                    if !can_read {
                        return Err(io::Error::new(
                            io::ErrorKind::PermissionDenied,
                            "fd not opened for read",
                        ));
                    }

                    vfs.read(fh, off, len).await.map_err(io::Error::from)
                }
                .await;

                let model_res: io::Result<Vec<u8>> = (|| {
                    let fd = match fds.model[slot].as_mut() {
                        Some(ModelFd::File(fd)) => fd,
                        _ => return Err(bad_fd()),
                    };
                    if !fd.read {
                        return Err(io::Error::new(
                            io::ErrorKind::PermissionDenied,
                            "fd not opened for read",
                        ));
                    }

                    fd.file.seek(SeekFrom::Start(off))?;
                    let mut buf = vec![0u8; len];
                    let n = fd.file.read(&mut buf)?;
                    buf.truncate(n);
                    Ok(buf)
                })();

                if let Some((got, expect)) = compare_outcome("pread", slayer_res, model_res) {
                    assert_eq!(got, expect, "pread: data mismatch at slot {slot}");
                }
            }
            Op::PWrite { slot, off, data } => {
                let slayer_res: io::Result<usize> = async {
                    let (fh, can_write, append) = match fds.slayer[slot].as_ref() {
                        Some(SlayerFd::File(fd)) => (fd.fh, fd.write, fd.append),
                        _ => return Err(bad_fd()),
                    };

                    if !can_write {
                        return Err(io::Error::new(
                            io::ErrorKind::PermissionDenied,
                            "fd not opened for write",
                        ));
                    }
                    if append {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "pwrite on append fd unsupported",
                        ));
                    }

                    vfs.write(fh, off, &data).await.map_err(io::Error::from)
                }
                .await;

                let model_res: io::Result<usize> = (|| {
                    let fd = match fds.model[slot].as_mut() {
                        Some(ModelFd::File(fd)) => fd,
                        _ => return Err(bad_fd()),
                    };
                    if !fd.write {
                        return Err(io::Error::new(
                            io::ErrorKind::PermissionDenied,
                            "fd not opened for write",
                        ));
                    }
                    if fd.append {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "pwrite on append fd unsupported",
                        ));
                    }

                    fd.file.seek(SeekFrom::Start(off))?;
                    fd.file.write_all(&data)?;
                    Ok(data.len())
                })();

                if let Some((got, expect)) = compare_outcome("pwrite", slayer_res, model_res) {
                    assert_eq!(got, expect, "pwrite: byte count mismatch at slot {slot}");
                }
            }
            Op::Create { path, exclusive } => {
                let sp = fs_path(&path);
                let mp = model_path(model_root.path(), &path);

                let slayer_res = vfs
                    .create_file_in_existing_dir_err(&sp, exclusive)
                    .await
                    .map(|_| ())
                    .map_err(io::Error::from);

                let model_res = {
                    let mut opts = OpenOptions::new();
                    opts.write(true);
                    if exclusive {
                        opts.create_new(true);
                    } else {
                        opts.create(true);
                    }
                    opts.open(&mp).map(|_| ())
                };

                let _ = compare_outcome("create", slayer_res, model_res);
            }
            Op::Unlink { path } => {
                let sp = fs_path(&path);
                let mp = model_path(model_root.path(), &path);

                let slayer_res = vfs.unlink(&sp).await.map_err(io::Error::from);
                let model_res = fs::remove_file(&mp);
                let _ = compare_outcome("unlink", slayer_res, model_res);
            }
            Op::Mkdir { path } => {
                let sp = fs_path(&path);
                let mp = model_path(model_root.path(), &path);

                let slayer_res = vfs
                    .mkdir_err(&sp)
                    .await
                    .map(|_| ())
                    .map_err(io::Error::from);
                let model_res = model_mkdir_like_slayer(&mp);
                let _ = compare_outcome("mkdir", slayer_res, model_res);
            }
            Op::Rmdir { path } => {
                let sp = fs_path(&path);
                let mp = model_path(model_root.path(), &path);

                let slayer_res = vfs.rmdir(&sp).await.map_err(io::Error::from);
                let model_res = fs::remove_dir(&mp);
                let _ = compare_outcome("rmdir", slayer_res, model_res);
            }
            Op::Readdir { slot } => {
                let slayer_res: io::Result<Vec<String>> = async {
                    let (fh, cur_off) = match fds.slayer[slot].as_ref() {
                        Some(SlayerFd::Dir(fd)) => (fd.fh, fd.offset),
                        _ => return Err(bad_fd()),
                    };

                    let mut names = vfs
                        .readdir(fh, cur_off)
                        .ok_or_else(bad_fd)?
                        .into_iter()
                        .map(|e| e.name)
                        .collect::<Vec<_>>();
                    names.sort_unstable();

                    if let Some(SlayerFd::Dir(fd)) = fds.slayer[slot].as_mut() {
                        fd.offset = cur_off + names.len() as u64;
                    }
                    Ok(names)
                }
                .await;

                let model_res: io::Result<Vec<String>> = (|| {
                    let fd = match fds.model[slot].as_mut() {
                        Some(ModelFd::Dir(fd)) => fd,
                        _ => return Err(bad_fd()),
                    };

                    let out = model_readdir_chunk(&fd.entries, fd.offset);
                    fd.offset += out.len() as u64;
                    Ok(out)
                })();

                if let Some((got, expect)) = compare_outcome("readdir", slayer_res, model_res)
                    && should_strict_compare("readdir")
                {
                    assert_eq!(got, expect, "readdir: entries mismatch at slot {slot}");
                }
            }
            Op::Readlink { path } => {
                let sp = fs_path(&path);
                let mp = model_path(model_root.path(), &path);

                let slayer_res = vfs.readlink(&sp).await.map_err(io::Error::from);
                let model_res = fs::read_link(&mp).map(|p| p.to_string_lossy().into_owned());

                if let Some((got, expect)) = compare_outcome("readlink", slayer_res, model_res)
                    && should_strict_compare("readlink")
                {
                    assert_eq!(got, expect, "readlink: target mismatch at {sp}");
                }
            }
            Op::Stat { path } => {
                let sp = fs_path(&path);
                let mp = model_path(model_root.path(), &path);

                let slayer_res = vfs.stat(&sp).await.map_err(io::Error::from);
                let model_res = fs::symlink_metadata(&mp);

                if let Some((sm, mm)) = compare_outcome("stat", slayer_res, model_res)
                    && should_strict_compare("stat")
                {
                    compare_attrs("stat", &sp, &sm, &mm);
                }
            }
            Op::Lstat { path } => {
                let sp = fs_path(&path);
                let mp = model_path(model_root.path(), &path);

                let slayer_res = vfs.stat(&sp).await.map_err(io::Error::from);
                let model_res = fs::symlink_metadata(&mp);

                if let Some((sm, mm)) = compare_outcome("lstat", slayer_res, model_res)
                    && should_strict_compare("lstat")
                {
                    compare_attrs("lstat", &sp, &sm, &mm);
                }
            }
            Op::Fstat { slot } => {
                let slayer_res: io::Result<VfsFileAttr> = async {
                    let path = match fds.slayer[slot].as_ref() {
                        Some(fd) => fd.path().to_string(),
                        None => return Err(bad_fd()),
                    };
                    vfs.stat(&path).await.map_err(io::Error::from)
                }
                .await;

                let model_res: io::Result<fs::Metadata> = match fds.model[slot].as_ref() {
                    Some(fd) => fs::symlink_metadata(fd.path()),
                    None => Err(bad_fd()),
                };

                if let Some((sm, mm)) = compare_outcome("fstat", slayer_res, model_res)
                    && should_strict_compare("fstat")
                {
                    compare_attrs("fstat", "<fd>", &sm, &mm);
                }
            }
            Op::Chmod { path, mode } => {
                let sp = fs_path(&path);
                let mp = model_path(model_root.path(), &path);

                let slayer_res: io::Result<VfsFileAttr> = async {
                    let attr = vfs.stat(&sp).await.map_err(io::Error::from)?;
                    let req = SetAttrRequest {
                        mode: Some(mode),
                        ..Default::default()
                    };
                    vfs.set_attr(attr.ino, &req, SetAttrFlags::empty())
                        .await
                        .map_err(io::Error::from)
                }
                .await;

                let model_res: io::Result<fs::Metadata> = (|| {
                    let perms = fs::Permissions::from_mode(mode);
                    fs::set_permissions(&mp, perms)?;
                    fs::symlink_metadata(&mp)
                })();

                if let Some((sm, mm)) = compare_outcome("chmod", slayer_res, model_res)
                    && should_strict_compare("chmod")
                {
                    compare_attrs("chmod", &sp, &sm, &mm);
                    assert_eq!(
                        sm.mode & 0o777,
                        mm.mode() & 0o777,
                        "chmod: mode mismatch at {sp}"
                    );
                }
            }
            Op::Fchmod { slot, mode } => {
                let slayer_res: io::Result<VfsFileAttr> = async {
                    let ino = match fds.slayer[slot].as_ref() {
                        Some(SlayerFd::File(fd)) => fd.ino,
                        _ => return Err(bad_fd()),
                    };

                    let req = SetAttrRequest {
                        mode: Some(mode),
                        ..Default::default()
                    };
                    vfs.set_attr(ino, &req, SetAttrFlags::empty())
                        .await
                        .map_err(io::Error::from)
                }
                .await;

                let model_res: io::Result<fs::Metadata> = (|| {
                    let fd = match fds.model[slot].as_mut() {
                        Some(ModelFd::File(fd)) => fd,
                        _ => return Err(bad_fd()),
                    };

                    fd.file.set_permissions(fs::Permissions::from_mode(mode))?;
                    fd.file.metadata()
                })();

                if let Some((sm, mm)) = compare_outcome("fchmod", slayer_res, model_res)
                    && should_strict_compare("fchmod")
                {
                    compare_attrs("fchmod", "<fd>", &sm, &mm);
                    assert_eq!(
                        sm.mode & 0o777,
                        mm.mode() & 0o777,
                        "fchmod: mode mismatch at slot {slot}"
                    );
                }
            }
            Op::Utimens {
                path,
                atime_ns,
                mtime_ns,
                atime_now,
                mtime_now,
            } => {
                let sp = fs_path(&path);
                let mp = model_path(model_root.path(), &path);

                let slayer_res: io::Result<()> = async {
                    let attr = vfs.stat(&sp).await.map_err(io::Error::from)?;
                    let mut req = SetAttrRequest::default();
                    let mut flags = SetAttrFlags::empty();

                    if atime_now {
                        flags |= SetAttrFlags::SET_ATIME_NOW;
                    } else {
                        req.atime = Some(atime_ns);
                    }

                    if mtime_now {
                        flags |= SetAttrFlags::SET_MTIME_NOW;
                    } else {
                        req.mtime = Some(mtime_ns);
                    }

                    vfs.set_attr(attr.ino, &req, flags)
                        .await
                        .map(|_| ())
                        .map_err(io::Error::from)
                }
                .await;

                let model_res = model_utimens(&mp, atime_ns, mtime_ns, atime_now, mtime_now);
                let _ = compare_outcome("utimens", slayer_res, model_res);
            }
            Op::Ftruncate { slot, size } => {
                let slayer_res: io::Result<()> = async {
                    let (ino, can_write, cur_off) = match fds.slayer[slot].as_ref() {
                        Some(SlayerFd::File(fd)) => (fd.ino, fd.write, fd.offset),
                        _ => return Err(bad_fd()),
                    };

                    if !can_write {
                        return Err(io::Error::new(
                            io::ErrorKind::PermissionDenied,
                            "fd not opened for write",
                        ));
                    }

                    vfs.truncate_inode(ino, size)
                        .await
                        .map_err(io::Error::from)?;

                    if let Some(SlayerFd::File(fd)) = fds.slayer[slot].as_mut()
                        && cur_off > size
                    {
                        fd.offset = size;
                    }
                    Ok(())
                }
                .await;

                let model_res: io::Result<()> = (|| {
                    let fd = match fds.model[slot].as_mut() {
                        Some(ModelFd::File(fd)) => fd,
                        _ => return Err(bad_fd()),
                    };

                    if !fd.write {
                        return Err(io::Error::new(
                            io::ErrorKind::PermissionDenied,
                            "fd not opened for write",
                        ));
                    }

                    fd.file.set_len(size)?;
                    if fd.offset > size {
                        fd.offset = size;
                    }
                    Ok(())
                })();

                let _ = compare_outcome("ftruncate", slayer_res, model_res);
            }
            Op::Fsync { slot } => {
                let slayer_res: io::Result<()> = async {
                    let fh = match fds.slayer[slot].as_ref() {
                        Some(SlayerFd::File(fd)) => fd.fh,
                        _ => return Err(bad_fd()),
                    };
                    vfs.fsync(fh, false).await.map_err(io::Error::from)
                }
                .await;

                let model_res: io::Result<()> = match fds.model[slot].as_mut() {
                    Some(ModelFd::File(fd)) => fd.file.sync_all(),
                    _ => Err(bad_fd()),
                };
                let _ = compare_outcome("fsync", slayer_res, model_res);
            }
            Op::Fdatasync { slot } => {
                let slayer_res: io::Result<()> = async {
                    let fh = match fds.slayer[slot].as_ref() {
                        Some(SlayerFd::File(fd)) => fd.fh,
                        _ => return Err(bad_fd()),
                    };
                    vfs.fsync(fh, true).await.map_err(io::Error::from)
                }
                .await;

                let model_res: io::Result<()> = match fds.model[slot].as_mut() {
                    Some(ModelFd::File(fd)) => fd.file.sync_data(),
                    _ => Err(bad_fd()),
                };
                let _ = compare_outcome("fdatasync", slayer_res, model_res);
            }
            Op::Symlink { src, dst } => {
                let src_sp = fs_path(&src);
                let dst_sp = fs_path(&dst);
                let dst_mp = model_path(model_root.path(), &dst);

                let slayer_res = vfs
                    .create_symlink(&dst_sp, &src_sp)
                    .await
                    .map(|_| ())
                    .map_err(io::Error::from);
                let model_res = model_symlink(&src_sp, &dst_mp);
                let _ = compare_outcome("symlink", slayer_res, model_res);
            }
            Op::Link { src, dst } => {
                let src_sp = fs_path(&src);
                let dst_sp = fs_path(&dst);
                let src_mp = model_path(model_root.path(), &src);
                let dst_mp = model_path(model_root.path(), &dst);

                let slayer_res = vfs
                    .link(&src_sp, &dst_sp)
                    .await
                    .map(|_| ())
                    .map_err(io::Error::from);
                let model_res = fs::hard_link(&src_mp, &dst_mp);
                let _ = compare_outcome("link", slayer_res, model_res);
            }
            Op::Rename { src, dst } => {
                let src_sp = fs_path(&src);
                let dst_sp = fs_path(&dst);
                let src_mp = model_path(model_root.path(), &src);
                let dst_mp = model_path(model_root.path(), &dst);

                let slayer_res = vfs.rename(&src_sp, &dst_sp).await.map_err(io::Error::from);
                let model_res = fs::rename(&src_mp, &dst_mp);
                if compare_outcome("rename", slayer_res, model_res).is_some() {
                    refresh_fd_paths_after_rename(&mut fds, &src_sp, &dst_sp, &src_mp, &dst_mp);
                }
            }
            Op::PurgeDir { dir, recreate } => {
                let root_sp = fs_path(&dir);
                let root_mp = model_path(model_root.path(), &dir);

                for slot in 0..MAX_FDS {
                    let should_close = matches!(fds.slayer[slot].as_ref(), Some(fd) if fd.path().starts_with(&root_sp));
                    if !should_close {
                        continue;
                    }

                    fds.assert_shape("purge-close");
                    let owner = slot_owner_or(&fds, slot, client_idx);
                    let slayer_res =
                        close_slayer_slot(&vfs_clients[owner], &mut fds.slayer[slot]).await;
                    let model_res = close_model_slot(&mut fds.model[slot]);
                    let _ = compare_outcome("purge-close", slayer_res, model_res);
                    fds.slot_owner[slot] = None;
                }

                for rel in subtree_unlink_targets(&dir) {
                    let sp = fs_path(&rel);
                    let mp = model_path(model_root.path(), &rel);
                    let slayer_res = vfs.unlink(&sp).await.map_err(io::Error::from);
                    let model_res = fs::remove_file(&mp);
                    let _ = compare_outcome("purge-unlink", slayer_res, model_res);
                }

                for rel in subtree_rmdir_targets(&dir) {
                    let sp = fs_path(&rel);
                    let mp = model_path(model_root.path(), &rel);
                    let slayer_res = vfs.rmdir(&sp).await.map_err(io::Error::from);
                    let model_res = fs::remove_dir(&mp);
                    let _ = compare_outcome("purge-rmdir-sub", slayer_res, model_res);
                }

                let slayer_res = vfs.rmdir(&root_sp).await.map_err(io::Error::from);
                let model_res = fs::remove_dir(&root_mp);
                let _ = compare_outcome("purge-rmdir-root", slayer_res, model_res);

                if recreate {
                    let slayer_res = vfs.mkdir_p(&root_sp).await.map_err(io::Error::from);
                    let model_res = fs::create_dir_all(&root_mp);
                    let _ = compare_outcome("purge-recreate", slayer_res, model_res);
                }
            }
        }

        if clients > 1 && i % 16 == 15 {
            close_all_slots(&vfs_clients, &mut fds, "barrier-close").await;
        } else if clients == 1 && i % 8 == 0 && !fds.has_file_fd() && !fds.has_dir_fd() {
            verify_all_clients(&vfs_clients, model_root.path()).await;
        }
    }

    close_all_slots(&vfs_clients, &mut fds, "final-close").await;

    if clients == 1 {
        verify_all_clients(&vfs_clients, model_root.path()).await;
    }
}

fuzz_target!(|data: &[u8]| {
    let rt = Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime");

    rt.block_on(async {
        run_case(data).await;
    });
});
