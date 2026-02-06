#![allow(clippy::unnecessary_cast)]
pub mod bind_mount;
pub mod mapping;
pub mod open_options;

use tracing::error;

use std::{fmt::Display, path::PathBuf};

#[cfg(target_os = "macos")]
use libc::stat as stat64;
#[cfg(target_os = "linux")]
use libc::stat64;
use rfuse3::{FileType, Timestamp, raw::reply::FileAttr};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct GPath {
    pub path: Vec<String>,
}

impl GPath {
    pub fn new() -> GPath {
        GPath { path: Vec::new() }
    }
    pub fn push(&mut self, path: String) {
        self.path.push(path);
    }
    pub fn pop(&mut self) -> Option<String> {
        self.path.pop()
    }
    pub fn name(&self) -> String {
        self.path.last().unwrap().clone()
    }
    pub fn part(&self, i: usize, j: usize) -> String {
        self.path[i..j].join("/")
    }
}

impl From<String> for GPath {
    fn from(mut s: String) -> GPath {
        if s.starts_with('/') {
            s.remove(0);
        }
        GPath {
            path: s.split('/').map(String::from).collect(),
        }
    }
}

impl From<GPath> for PathBuf {
    fn from(val: GPath) -> Self {
        let path_str = val.path.join("/");
        PathBuf::from(path_str)
    }
}
impl Display for GPath {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.path.join("/"))
    }
}

pub fn convert_stat64_to_file_attr(stat: stat64) -> FileAttr {
    FileAttr {
        ino: stat.st_ino,
        size: stat.st_size as u64,
        blocks: stat.st_blocks as u64,
        atime: Timestamp::new(stat.st_atime, stat.st_atime_nsec.try_into().unwrap()),
        mtime: Timestamp::new(stat.st_mtime, stat.st_mtime_nsec.try_into().unwrap()),
        ctime: Timestamp::new(stat.st_ctime, stat.st_ctime_nsec.try_into().unwrap()),
        #[cfg(target_os = "macos")]
        crtime: Timestamp::new(0, 0), // Set crtime to 0 for non-macOS platforms
        kind: filetype_from_mode(stat.st_mode as u32),
        perm: (stat.st_mode & 0o7777) as u16,
        nlink: stat.st_nlink as u32,
        uid: stat.st_uid,
        gid: stat.st_gid,
        rdev: stat.st_rdev as u32,
        #[cfg(target_os = "macos")]
        flags: 0, // Set flags to 0 for non-macOS platforms
        blksize: stat.st_blksize as u32,
    }
}

pub fn filetype_from_mode(st_mode: u32) -> FileType {
    let st_mode = st_mode & (libc::S_IFMT as u32);
    if st_mode == (libc::S_IFIFO as u32) {
        return FileType::NamedPipe;
    }
    if st_mode == (libc::S_IFCHR as u32) {
        return FileType::CharDevice;
    }
    if st_mode == (libc::S_IFBLK as u32) {
        return FileType::BlockDevice;
    }
    if st_mode == (libc::S_IFDIR as u32) {
        return FileType::Directory;
    }
    if st_mode == (libc::S_IFREG as u32) {
        return FileType::RegularFile;
    }
    if st_mode == (libc::S_IFLNK as u32) {
        return FileType::Symlink;
    }
    if st_mode == (libc::S_IFSOCK as u32) {
        return FileType::Socket;
    }
    // Handle whiteout files on macOS (0xE000 / 57344)
    // rfuse3 doesn't seem to have a specific Whiteout variant exposed or we don't have it imported.
    // Treating as regular file or simply not panicking.
    // Ideally we should filter these out if they are not real files, or map to closest.
    #[cfg(target_os = "macos")]
    if st_mode == 0xE000 {
        return FileType::RegularFile;
    }
    error!("wrong st mode : {st_mode}");
    unreachable!();
}
#[cfg(test)]
mod tests {
    use super::GPath;

    #[test]
    fn test_from_string() {
        let path = String::from("/release");
        let gapth = GPath::from(path);
        assert_eq!(gapth.to_string(), String::from("release"))
    }
}
