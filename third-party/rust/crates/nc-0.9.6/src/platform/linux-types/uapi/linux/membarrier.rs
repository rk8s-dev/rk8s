// Copyright (c) 2020 Xu Shaohua <shaohua@biofan.org>. All rights reserved.
// Use of this source is governed by Apache-2.0 License that can be found
// in the LICENSE file.

//! From `include/uapi/linux/membarrier.h`

/// enum `membarrier_cmd` - membarrier system call command
///
/// @`MEMBARRIER_CMD_QUERY`:   Query the set of supported commands. It returns
///                          a bitmask of valid commands.
/// @`MEMBARRIER_CMD_GLOBAL`:  Execute a memory barrier on all running threads.
///                          Upon return from system call, the caller thread
///                          is ensured that all running threads have passed
///                          through a state where all memory accesses to
///                          user-space addresses match program order between
///                          entry to and return from the system call
///                          (non-running threads are de facto in such a
///                          state). This covers threads from all processes
///                          running on the system. This command returns 0.
/// @`MEMBARRIER_CMD_GLOBAL_EXPEDITED`:
///                          Execute a memory barrier on all running threads
///                          of all processes which previously registered
///                          with `MEMBARRIER_CMD_REGISTER_GLOBAL_EXPEDITED`.
///                          Upon return from system call, the caller thread
///                          is ensured that all running threads have passed
///                          through a state where all memory accesses to
///                          user-space addresses match program order between
///                          entry to and return from the system call
///                          (non-running threads are de facto in such a
///                          state). This only covers threads from processes
///                          which registered with
///                          `MEMBARRIER_CMD_REGISTER_GLOBAL_EXPEDITED`.
///                          This command returns 0. Given that
///                          registration is about the intent to receive
///                          the barriers, it is valid to invoke
///                          `MEMBARRIER_CMD_GLOBAL_EXPEDITED` from a
///                          non-registered process.
/// @`MEMBARRIER_CMD_REGISTER_GLOBAL_EXPEDITED`:
///                          Register the process intent to receive
///                          `MEMBARRIER_CMD_GLOBAL_EXPEDITED` memory
///                          barriers. Always returns 0.
/// @`MEMBARRIER_CMD_PRIVATE_EXPEDITED`:
///                          Execute a memory barrier on each running
///                          thread belonging to the same process as the current
///                          thread. Upon return from system call, the
///                          caller thread is ensured that all its running
///                          threads siblings have passed through a state
///                          where all memory accesses to user-space
///                          addresses match program order between entry
///                          to and return from the system call
///                          (non-running threads are de facto in such a
///                          state). This only covers threads from the
///                          same process as the caller thread. This
///                          command returns 0 on success. The
///                          "expedited" commands complete faster than
///                          the non-expedited ones, they never block,
///                          but have the downside of causing extra
///                          overhead. A process needs to register its
///                          intent to use the private expedited command
///                          prior to using it, otherwise this command
///                          returns -EPERM.
/// @`MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED`:
///                          Register the process intent to use
///                          `MEMBARRIER_CMD_PRIVATE_EXPEDITED`. Always
///                          returns 0.
/// @`MEMBARRIER_CMD_PRIVATE_EXPEDITED_SYNC_CORE`:
///                          In addition to provide memory ordering
///                          guarantees described in
///                          `MEMBARRIER_CMD_PRIVATE_EXPEDITED`, ensure
///                          the caller thread, upon return from system
///                          call, that all its running threads siblings
///                          have executed a core serializing
///                          instruction. (architectures are required to
///                          guarantee that non-running threads issue
///                          core serializing instructions before they
///                          resume user-space execution). This only
///                          covers threads from the same process as the
///                          caller thread. This command returns 0 on
///                          success. The "expedited" commands complete
///                          faster than the non-expedited ones, they
///                          never block, but have the downside of
///                          causing extra overhead. If this command is
///                          not implemented by an architecture, -EINVAL
///                          is returned. A process needs to register its
///                          intent to use the private expedited sync
///                          core command prior to using it, otherwise
///                          this command returns -EPERM.
/// @`MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED_SYNC_CORE`:
///                          Register the process intent to use
///                          `MEMBARRIER_CMD_PRIVATE_EXPEDITED_SYNC_CORE`.
///                          If this command is not implemented by an
///                          architecture, -EINVAL is returned.
///                          Returns 0 on success.
/// @`MEMBARRIER_CMD_SHARED`:
///                          Alias to `MEMBARRIER_CMD_GLOBAL`. Provided for
///                          header backward compatibility.
///
/// Command to be passed to the membarrier system call. The commands need to
/// be a single bit each, except for `MEMBARRIER_CMD_QUERY` which is assigned to
/// the value 0.
pub const MEMBARRIER_CMD_QUERY: i32 = 0;
pub const MEMBARRIER_CMD_GLOBAL: i32 = 1;
pub const MEMBARRIER_CMD_GLOBAL_EXPEDITED: i32 = 1 << 1;
pub const MEMBARRIER_CMD_REGISTER_GLOBAL_EXPEDITED: i32 = 1 << 2;
pub const MEMBARRIER_CMD_PRIVATE_EXPEDITED: i32 = 1 << 3;
pub const MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED: i32 = 1 << 4;
pub const MEMBARRIER_CMD_PRIVATE_EXPEDITED_SYNC_CORE: i32 = 1 << 5;
pub const MEMBARRIER_CMD_REGISTER_PRIVATE_EXPEDITED_SYNC_CORE: i32 = 1 << 6;
/// Alias for header backward compatibility.
pub const MEMBARRIER_CMD_SHARED: i32 = MEMBARRIER_CMD_GLOBAL;
