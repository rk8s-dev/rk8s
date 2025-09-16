// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/futex.h`

#![allow(clippy::module_name_repetitions)]

/// Second argument to futex syscall
pub const FUTEX_WAIT: i32 = 0;
pub const FUTEX_WAKE: i32 = 1;
pub const FUTEX_FD: i32 = 2;
pub const FUTEX_REQUEUE: i32 = 3;
pub const FUTEX_CMP_REQUEUE: i32 = 4;
pub const FUTEX_WAKE_OP: i32 = 5;
pub const FUTEX_LOCK_PI: i32 = 6;
pub const FUTEX_UNLOCK_PI: i32 = 7;
pub const FUTEX_TRYLOCK_PI: i32 = 8;
pub const FUTEX_WAIT_BITSET: i32 = 9;
pub const FUTEX_WAKE_BITSET: i32 = 10;
pub const FUTEX_WAIT_REQUEUE_PI: i32 = 11;
pub const FUTEX_CMP_REQUEUE_PI: i32 = 12;

pub const FUTEX_PRIVATE_FLAG: i32 = 128;
pub const FUTEX_CLOCK_REALTIME: i32 = 256;
pub const FUTEX_CMD_MASK: i32 = !FUTEX_PRIVATE_FLAG | FUTEX_CLOCK_REALTIME;

pub const FUTEX_WAIT_PRIVATE: i32 = FUTEX_WAIT | FUTEX_PRIVATE_FLAG;
pub const FUTEX_WAKE_PRIVATE: i32 = FUTEX_WAKE | FUTEX_PRIVATE_FLAG;
pub const FUTEX_REQUEUE_PRIVATE: i32 = FUTEX_REQUEUE | FUTEX_PRIVATE_FLAG;
pub const FUTEX_CMP_REQUEUE_PRIVATE: i32 = FUTEX_CMP_REQUEUE | FUTEX_PRIVATE_FLAG;
pub const FUTEX_WAKE_OP_PRIVATE: i32 = FUTEX_WAKE_OP | FUTEX_PRIVATE_FLAG;
pub const FUTEX_LOCK_PI_PRIVATE: i32 = FUTEX_LOCK_PI | FUTEX_PRIVATE_FLAG;
pub const FUTEX_UNLOCK_PI_PRIVATE: i32 = FUTEX_UNLOCK_PI | FUTEX_PRIVATE_FLAG;
pub const FUTEX_TRYLOCK_PI_PRIVATE: i32 = FUTEX_TRYLOCK_PI | FUTEX_PRIVATE_FLAG;
pub const FUTEX_WAIT_BITSET_PRIVATE: i32 = FUTEX_WAIT_BITSET | FUTEX_PRIVATE_FLAG;
pub const FUTEX_WAKE_BITSET_PRIVATE: i32 = FUTEX_WAKE_BITSET | FUTEX_PRIVATE_FLAG;
pub const FUTEX_WAIT_REQUEUE_PI_PRIVATE: i32 = FUTEX_WAIT_REQUEUE_PI | FUTEX_PRIVATE_FLAG;
pub const FUTEX_CMP_REQUEUE_PI_PRIVATE: i32 = FUTEX_CMP_REQUEUE_PI | FUTEX_PRIVATE_FLAG;

/// Flags to specify the bit length of the futex word for futex2 syscalls.
/// Currently, only 32 is supported.
pub const FUTEX_32: i32 = 2;

/// Max numbers of elements in a `futex_waitv_t` array.
pub const FUTEX_WAITV_MAX: i32 = 128;

/// A waiter for vectorized wait.
#[repr(C)]
#[derive(Debug)]
pub struct futex_waitv_t {
    /// Expected value at uaddr.
    pub val: u64,
    /// User address to wait on.
    pub uaddr: u64,
    /// Flags for this waiter.
    pub flags: u32,
    /// Reserved member to preserve data alignment. Should be 0.
    __reserved: u32,
}

/// Support for robust futexes: the kernel cleans up held futexes at thread exit time.
///
/// Per-lock list entry - embedded in user-space locks, somewhere close
/// to the futex field. (Note: user-space uses a double-linked list to
/// achieve O(1) list add and remove, but the kernel only needs to know
/// about the forward link)
///
/// NOTE: this structure is part of the syscall ABI, and must not be changed.
#[repr(C)]
#[derive(Debug)]
pub struct robust_list_t {
    pub next: *mut robust_list_t,
}

/// Per-thread list head:
///
/// NOTE: this structure is part of the syscall ABI, and must only be
/// changed if the change is first communicated with the glibc folks.
/// (When an incompatible change is done, we'll increase the structure
/// size, which glibc will detect)
#[repr(C)]
#[derive(Debug)]
pub struct robust_list_head_t {
    /// The head of the list. Points back to itself if empty:
    pub list: robust_list_t,

    /// This relative offset is set by user-space, it gives the kernel
    /// the relative position of the futex field to examine. This way
    /// we keep userspace flexible, to freely shape its data-structure,
    /// without hardcoding any particular offset into the kernel:
    pub futex_offset: isize,

    /// The death of the thread may race with userspace setting
    /// up a lock's links. So to handle this race, userspace first
    /// sets this field to the address of the to-be-taken lock,
    /// then does the lock acquire, and then adds itself to the
    /// list, and then clears this field. Hence the kernel will
    /// always have full knowledge of all locks that the thread
    /// _might_ have taken. We check the owner TID in any case,
    /// so only truly owned locks will be handled.
    pub list_op_pending: *mut robust_list_t,
}

/// Are there any waiters for this robust futex:
#[allow(overflowing_literals)]
pub const FUTEX_WAITERS: i32 = 0x8000_0000;

/// The kernel signals via this bit that a thread holding a futex
/// has exited without unlocking the futex.
///
/// The kernel also does a `FUTEX_WAKE` on such futexes, after setting the bit,
/// to wake up any possible waiters:
pub const FUTEX_OWNER_DIED: i32 = 0x4000_0000;

/// The rest of the robust-futex field is for the TID:
pub const FUTEX_TID_MASK: i32 = 0x3fff_ffff;

/// This limit protects against a deliberately circular list.
/// (Not worth introducing an rlimit for it)
pub const ROBUST_LIST_LIMIT: i32 = 2048;

/// bitset with all bits set for the `FUTEX_xxx_BITSET` OPs to request a
/// match of any bit.
#[allow(overflowing_literals)]
pub const FUTEX_BITSET_MATCH_ANY: i32 = 0xffff_ffff;

/// *(int *)UADDR2 = OPARG;
pub const FUTEX_OP_SET: i32 = 0;
/// *(int *)UADDR2 += OPARG;
pub const FUTEX_OP_ADD: i32 = 1;
/// *(int *)UADDR2 |= OPARG;
pub const FUTEX_OP_OR: i32 = 2;
/// *(int *)UADDR2 &= ~OPARG;
pub const FUTEX_OP_ANDN: i32 = 3;
/// *(int *)UADDR2 ^= OPARG;
pub const FUTEX_OP_XOR: i32 = 4;

/// Use (1 << OPARG) instead of OPARG.
pub const FUTEX_OP_OPARG_SHIFT: i32 = 8;

/// if (oldval == CMPARG) wake
pub const FUTEX_OP_CMP_EQ: i32 = 0;
/// if (oldval != CMPARG) wake
pub const FUTEX_OP_CMP_NE: i32 = 1;
/// if (oldval < CMPARG) wake
pub const FUTEX_OP_CMP_LT: i32 = 2;
/// if (oldval <= CMPARG) wake
pub const FUTEX_OP_CMP_LE: i32 = 3;
/// if (oldval > CMPARG) wake
pub const FUTEX_OP_CMP_GT: i32 = 4;
/// if (oldval >= CMPARG) wake
pub const FUTEX_OP_CMP_GE: i32 = 5;

/// `FUTEX_WAKE_OP` will perform atomically.
///
/// ```c
/// int oldval = *(int *)UADDR2;
/// *(int *)UADDR2 = oldval OP OPARG;
/// if (oldval CMP CMPARG)
/// wake UADDR2;
/// ```
#[inline]
#[must_use]
pub const fn FUTEX_OP(op: i32, oparg: i32, cmp: i32, cmparg: i32) -> i32 {
    ((op & 0xf) << 28) | ((cmp & 0xf) << 24) | ((oparg & 0xfff) << 12) | (cmparg & 0xfff)
}
