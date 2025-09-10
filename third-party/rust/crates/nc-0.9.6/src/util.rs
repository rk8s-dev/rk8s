// Copyright (c) 2022 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

#![allow(clippy::missing_errors_doc)]
#![allow(clippy::cast_sign_loss)]

use alloc::collections::BTreeSet;
#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use crate::Errno;

pub type SyscallTable = BTreeSet<String>;

pub const ALL_SYMS: &str = "/proc/kallsyms";

/// A simple wrapper to File IO.
///
/// File is closed when drop.
pub struct File {
    fd: i32,
}

impl File {
    /// Open file readonly.
    pub fn open(path: &str) -> Result<Self, crate::Errno> {
        let fd: i32 = unsafe { crate::openat(crate::AT_FDCWD, path, crate::O_RDONLY, 0o600)? };

        Ok(Self { fd })
    }

    #[must_use]
    pub const fn fd(&self) -> i32 {
        self.fd
    }
}

impl Drop for File {
    fn drop(&mut self) {
        if self.fd > -1 {
            let _ret = unsafe { crate::close(self.fd) };
            self.fd = -1;
        }
    }
}

/// Check syscall name exists in current system.
pub fn syscall_exists(name: &str) -> Result<bool, Errno> {
    const BUF_LEN: usize = 1024;

    let file = File::open(ALL_SYMS)?;
    let mut buf = [0_u8; BUF_LEN];
    let mut line_str = Vec::with_capacity(BUF_LEN);
    loop {
        let n_read = unsafe { crate::read(file.fd(), &mut buf) };
        match n_read {
            Err(errno) => return Err(errno),
            Ok(0) => break,
            Ok(n) => {
                let n = n as usize;
                for byte in &buf[0..n] {
                    if *byte != b'\n' {
                        line_str.push(*byte);
                        continue;
                    }

                    // Check this line
                    let s = alloc::str::from_utf8(&line_str).map_err(|_| crate::EIO)?;
                    let mut iter = s.split_ascii_whitespace();
                    if iter.next().is_some() && iter.next() == Some("T") {
                        if let Some(func_name) = iter.next() {
                            if let Some(pos) = func_name.find("sys_") {
                                if func_name.len() > pos + 4
                                    && iter.next().is_none()
                                    && &func_name[pos + 4..] == name
                                {
                                    return Ok(true);
                                }
                            }
                        }
                    }

                    // Reset last line
                    line_str.clear();
                }
            }
        }
    }

    Ok(false)
}

fn parse_syscall_from_sym(s: &str) -> Option<String> {
    let mut iter = s.split_ascii_whitespace();
    if iter.next().is_some() && iter.next() == Some("T") {
        if let Some(func_name) = iter.next() {
            if let Some(pos) = func_name.find("sys_") {
                if func_name.len() > pos + 4 && iter.next().is_none() {
                    return Some(func_name[pos + 4..].to_string());
                }
            }
        }
    }
    None
}

/// Read syscall list from /proc.
pub fn read_syscall_list() -> Result<SyscallTable, Errno> {
    const BUF_LEN: usize = 1024;

    let file = File::open(ALL_SYMS)?;
    let mut set = BTreeSet::new();
    let mut buf = [0_u8; BUF_LEN];
    let mut line_str = Vec::with_capacity(BUF_LEN);
    loop {
        let n_read = unsafe { crate::read(file.fd(), &mut buf) };
        match n_read {
            Err(errno) => return Err(errno),
            Ok(0) => break,
            Ok(n) => {
                let n = n as usize;
                for byte in &buf[0..n] {
                    if *byte != b'\n' {
                        line_str.push(*byte);
                        continue;
                    }

                    // Check this line
                    let s = alloc::str::from_utf8(&line_str).map_err(|_| crate::EIO)?;
                    if let Some(func_name) = parse_syscall_from_sym(s) {
                        #[allow(clippy::set_contains_or_insert)]
                        if !set.contains(&func_name) {
                            set.insert(func_name);
                        }
                    }

                    // Reset last line
                    line_str.clear();
                }
            }
        }
    }

    Ok(set)
}

#[cfg(test)]
mod tests {
    use super::{read_syscall_list, syscall_exists};

    #[test]
    fn test_read_syscall_list() {
        let list = read_syscall_list();
        assert!(list.is_ok());
        let list = list.unwrap();
        assert!(!list.is_empty());
    }

    #[test]
    fn test_syscall_exists() {
        let openat = "openat";
        assert_eq!(syscall_exists(openat), Ok(true));
    }
}
