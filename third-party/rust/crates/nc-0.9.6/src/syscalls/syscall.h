// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

#ifndef NC_SYSCALLS_SYSCALL_H
#define NC_SYSCALLS_SYSCALL_H

extern long __syscall0(long n);
extern long __syscall1(long n, long a1);
extern long __syscall2(long n, long a1, long a2);
extern long __syscall3(long n, long a1, long a2, long a3);
extern long __syscall4(long n, long a1, long a2, long a3, long a4);
extern long __syscall5(long n, long a1, long a2, long a3, long a4, long a5);
extern long __syscall6(long n, long a1, long a2, long a3, long a4, long a5, long a6);

#endif  // NC_SYSCALLS_SYSCALL_H
