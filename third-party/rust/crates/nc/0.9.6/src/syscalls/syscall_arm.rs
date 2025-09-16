// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::module_name_repetitions)]

use core::arch::asm;

use super::types::{check_errno, Errno, Sysno};

#[inline]
pub unsafe fn syscall0(n: Sysno) -> Result<usize, Errno> {
    let ret: usize;
    asm!("swi 0",
         in("r7") n,
         lateout("r0") ret,
    );
    check_errno(ret)
}

#[inline]
pub unsafe fn syscall1(n: Sysno, a1: usize) -> Result<usize, Errno> {
    let ret: usize;
    asm!("swi 0",
         in("r7") n,
         in("r0") a1,
         lateout("r0") ret,
    );
    check_errno(ret)
}

#[inline]
pub unsafe fn syscall2(n: Sysno, a1: usize, a2: usize) -> Result<usize, Errno> {
    let ret: usize;
    asm!("swi 0",
         in("r7") n,
         in("r0") a1,
         in("r1") a2,
         lateout("r0") ret,
    );
    check_errno(ret)
}

#[inline]
pub unsafe fn syscall3(n: Sysno, a1: usize, a2: usize, a3: usize) -> Result<usize, Errno> {
    let ret: usize;
    asm!("swi 0",
         in("r7") n,
         in("r0") a1,
         in("r1") a2,
         in("r2") a3,
         lateout("r0") ret,
    );
    check_errno(ret)
}

#[inline]
pub unsafe fn syscall4(
    n: Sysno,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
) -> Result<usize, Errno> {
    let ret: usize;
    asm!("swi 0",
         in("r7") n,
         in("r0") a1,
         in("r1") a2,
         in("r2") a3,
         in("r3") a4,
         lateout("r0") ret,
    );
    check_errno(ret)
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
    let ret: usize;
    asm!("swi 0",
         in("r7") n,
         in("r0") a1,
         in("r1") a2,
         in("r2") a3,
         in("r3") a4,
         in("r4") a5,
         lateout("r0") ret,
    );
    check_errno(ret)
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
    let ret: usize;
    asm!("swi 0",
         in("r7") n,
         in("r0") a1,
         in("r1") a2,
         in("r2") a3,
         in("r3") a4,
         in("r4") a5,
         in("r5") a6,
         lateout("r0") ret,
    );
    check_errno(ret)
}
