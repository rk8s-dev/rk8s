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
    asm!("int 0x80",
         in("eax") n,
         lateout("eax") ret,
    );
    check_errno(ret)
}

#[inline]
pub unsafe fn syscall1(n: Sysno, a1: usize) -> Result<usize, Errno> {
    let ret: usize;
    asm!("int 0x80",
         in("eax") n,
         in("ebx") a1,
         lateout("eax") ret,
    );
    check_errno(ret)
}

#[inline]
pub unsafe fn syscall2(n: Sysno, a1: usize, a2: usize) -> Result<usize, Errno> {
    let ret: usize;
    asm!("int 0x80",
         in("eax") n,
         in("ebx") a1,
         in("ecx") a2,
         lateout("eax") ret,
    );
    check_errno(ret)
}

#[inline]
pub unsafe fn syscall3(n: Sysno, a1: usize, a2: usize, a3: usize) -> Result<usize, Errno> {
    let ret: usize;
    asm!("int 0x80",
         in("eax") n,
         in("ebx") a1,
         in("ecx") a2,
         in("edx") a3,
         lateout("eax") ret,
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
    asm!("xchg esi, {a4}
         int 0x80
         xchg esi, {a4}",
         a4 = in(reg) a4,
         in("eax") n,
         in("ebx") a1,
         in("ecx") a2,
         in("edx") a3,
         lateout("eax") ret,
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
    asm!("xchg esi, {a4}
         int 0x80
         xchg esi, {a4}",
         a4 = in(reg) a4,
         in("eax") n,
         in("ebx") a1,
         in("ecx") a2,
         in("edx") a3,
         in("edi") a5,
         lateout("eax") ret,
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
    // ESI and EBP are reserved by LLVM.
    let ret: usize;
    asm!("xchg esi, {a4}
         xchg ebp, {a6}
         int 0x80
         xchg esi, {a4}
         xchg ebp, {a6}",
         a4 = in(reg) a4,
         a6 = in(reg) a6,
         in("eax") n,
         in("ebx") a1,
         in("ecx") a2,
         in("edx") a3,
         in("edi") a5,
         lateout("eax") ret,
    );
    check_errno(ret)
}
