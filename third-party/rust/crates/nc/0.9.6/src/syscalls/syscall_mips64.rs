// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::module_name_repetitions)]

use super::types::{check_errno, Errno, Sysno};

#[inline]
pub unsafe fn syscall0(n: Sysno) -> Result<usize, Errno> {
    let ret: usize;
    let mut n = n;
    llvm_asm!("syscall"
         : "+&{r2}"(n), "={r7}"(ret)
         :
         : "$1", "$3", "$10", "$11", "$12", "$13", "$14", "$15", "$24", "$25", "hi", "lo", "memory"
         : "volatile");
    check_errno(n)
}

#[inline]
pub unsafe fn syscall1(n: Sysno, a1: usize) -> Result<usize, Errno> {
    let ret: usize;
    let mut n = n;
    llvm_asm!("syscall"
         : "+&{r2}"(n), "={r7}"(ret)
         : "{r4}"(a1)
         : "$1", "$3", "$10", "$11", "$12", "$13", "$14", "$15", "$24", "$25", "hi", "lo", "memory"
         : "volatile");
    check_errno(n)
}

#[inline]
pub unsafe fn syscall2(n: Sysno, a1: usize, a2: usize) -> Result<usize, Errno> {
    let ret: usize;
    let mut n = n;
    llvm_asm!("syscall"
         : "+&{r2}"(n), "={r7}"(ret)
         : "{r4}"(a1),
           "{r5}"(a2)
         : "$1", "$3", "$10", "$11", "$12", "$13", "$14", "$15", "$24", "$25", "hi", "lo", "memory"
         : "volatile");
    check_errno(n)
}

#[inline]
pub unsafe fn syscall3(n: Sysno, a1: usize, a2: usize, a3: usize) -> Result<usize, Errno> {
    let ret: usize;
    let mut n = n;
    llvm_asm!("syscall"
         : "+&{r2}"(n), "={r7}"(ret)
         : "{r4}"(a1),
           "{r5}"(a2),
           "{r6}"(a3)
         : "$1", "$3", "$10", "$11", "$12", "$13", "$14", "$15", "$24", "$25", "hi", "lo", "memory"
         : "volatile");
    check_errno(n)
}

#[inline]
pub unsafe fn syscall4(
    n: Sysno,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
) -> Result<usize, Errno> {
    let mut n = n;
    let mut a4 = a4;
    llvm_asm!("syscall"
         : "+&{r2}"(n), "+{r7}"(a4)
         : "{r4}"(a1),
           "{r5}"(a2),
           "{r6}"(a3)
         : "$1", "$3", "$10", "$11", "$12", "$13", "$14", "$15", "$24", "$25", "hi", "lo", "memory"
         : "volatile");
    check_errno(n)
}

#[inline]
pub unsafe fn syscall5(
    n: Sysno,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
) -> Result<usize, Errno> {
    let mut n = n;
    let mut a4 = a4;
    llvm_asm!("syscall"
         : "+&{r2}"(n), "+{r7}"(a4)
         : "{r4}"(a1),
           "{r5}"(a2),
           "{r6}"(a3),
           "{r8}"(a5)
         : "$1", "$3", "$10", "$11", "$12", "$13", "$14", "$15", "$24", "$25", "hi", "lo", "memory"
         : "volatile");
    check_errno(n)
}

#[inline]
pub unsafe fn syscall6(
    n: Sysno,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
    a6: usize,
) -> Result<usize, Errno> {
    let mut n = n;
    let mut a4 = a4;
    llvm_asm!("syscall"
         : "+&{r2}"(n), "+{r7}"(a4)
         : "{r4}"(a1),
           "{r5}"(a2),
           "{r6}"(a3),
           "{r8}"(a5),
           "{r9}"(a6)
         : "$1", "$3", "$10", "$11", "$12", "$13", "$14", "$15", "$24", "$25", "hi", "lo", "memory"
         : "volatile");
    check_errno(n)
}
