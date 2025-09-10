// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::module_name_repetitions)]

use super::types::{check_errno, Errno, Sysno};

#[inline]
pub unsafe fn syscall0(n: Sysno) -> Result<usize, Errno> {
    let mut r0 = n;
    let r3: usize;
    llvm_asm!("sc"
         : "+{r0}"(r0), "={r3}"(r3)
         :
         : "memory", "cr0", "r4", "5", "r6", "r7", "r8", "r9", "r10", "r11", "r12"
         : "volatile");
    check_errno(r3)
}

#[inline]
pub unsafe fn syscall1(n: Sysno, a1: usize) -> Result<usize, Errno> {
    let mut r0 = n;
    let mut r3 = a1;
    llvm_asm!("sc"
         : "+{r0}"(r0), "+{r3}"(r3)
         :
         : "memory", "cr0", "r4", "5", "r6", "r7", "r8", "r9", "r10", "r11", "r12"
         : "volatile");
    check_errno(r3)
}

#[inline]
pub unsafe fn syscall2(n: Sysno, a1: usize, a2: usize) -> Result<usize, Errno> {
    let mut r0 = n;
    let mut r3 = a1;
    let mut r4 = a2;
    llvm_asm!("sc"
         : "+{r0}"(r0), "+{r3}"(r3), "+{r4}"(r4)
         :
         : "memory", "cr0", "r4", "5", "r6", "r7", "r8", "r9", "r10", "r11", "r12"
         : "volatile");
    check_errno(r3)
}

#[inline]
pub unsafe fn syscall3(n: Sysno, a1: usize, a2: usize, a3: usize) -> Result<usize, Errno> {
    let mut r0 = n;
    let mut r3 = a1;
    let mut r4 = a2;
    let mut r5 = a3;
    llvm_asm!("sc"
         : "+{r0}"(r0), "+{r3}"(r3), "+{r4}"(r4), "+{r5}"(r5)
         :
         : "memory", "cr0", "r4", "5", "r6", "r7", "r8", "r9", "r10", "r11", "r12"
         : "volatile");
    check_errno(r3)
}

#[inline]
pub unsafe fn syscall4(
    n: Sysno,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
) -> Result<usize, Errno> {
    let mut r0 = n;
    let mut r3 = a1;
    let mut r4 = a2;
    let mut r5 = a3;
    let mut r6 = a4;
    llvm_asm!("sc"
         : "+{r0}"(r0), "+{r3}"(r3), "+{r4}"(r4), "+{r5}"(r5), "+{r6}"(r6)
         :
         : "memory", "cr0", "r4", "5", "r6", "r7", "r8", "r9", "r10", "r11", "r12"
         : "volatile");
    check_errno(r3)
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
    let mut r0 = n;
    let mut r3 = a1;
    let mut r4 = a2;
    let mut r5 = a3;
    let mut r6 = a4;
    let mut r7 = a5;
    llvm_asm!("sc"
         : "+{r0}"(r0), "+{r3}"(r3), "+{r4}"(r4), "+{r5}"(r5), "+{r6}"(r6), "+{r7}"(r7)
         :
         : "memory", "cr0", "r4", "5", "r6", "r7", "r8", "r9", "r10", "r11", "r12"
         : "volatile");
    check_errno(r3)
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
    let mut r0 = n;
    let mut r3 = a1;
    let mut r4 = a2;
    let mut r5 = a3;
    let mut r6 = a4;
    let mut r7 = a5;
    let mut r8 = a6;
    llvm_asm!("sc"
         : "+{r0}"(r0), "+{r3}"(r3), "+{r4}"(r4), "+{r5}"(r5), "+{r6}"(r6), "+{r7}"(r7), "+{r8}"(r8)
         :
         : "memory", "cr0", "r4", "5", "r6", "r7", "r8", "r9", "r10", "r11", "r12"
         : "volatile");
    check_errno(r3)
}
