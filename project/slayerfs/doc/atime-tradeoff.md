# atime Tradeoff Note

## Background

`generic/192` checks strict access-time (`atime`) update behavior:

- reading a file should advance `atime`
- the updated `atime` should persist across remount

SlayerFS currently does not eagerly update `atime` on ordinary file reads, so this case fails.

## Why this is not a free fix

Making `atime` strictly correct in the naive way means:

- every successful read may trigger a metadata write
- that metadata write may hit Redis / database / etcd backend
- frequent reads on hot files turn into frequent metadata mutations
- cache invalidation pressure increases
- distributed deployments pay extra latency and write amplification

For a network / distributed filesystem, this can become a meaningful performance cost.

## Practical impact

Strict `atime` semantics are usually lower priority than:

- data correctness
- truncate / mmap visibility correctness
- lock correctness
- directory offset stability

This means `generic/192` is not currently the best repair target if the goal is to keep fixing higher-value semantic bugs first.

## Current decision

Temporarily defer `generic/192`.

Do not change xfstests or fake the result.
Instead, treat it as a known tradeoff until we decide one of these directions:

1. Full strict `atime`

- update metadata on every read
- highest semantic fidelity
- highest metadata overhead

2. Relaxed / policy-driven `atime`

- update only under a mount/config policy
- examples: `noatime`, `relatime`, batched/lazy `atime`
- lower overhead
- may not satisfy strict xfstests expectations

3. Deferred `atime`

- batch or coalesce updates asynchronously
- better performance than per-read writes
- more complex correctness and persistence story

## Recommendation

For now:

- skip working on `generic/192`
- continue fixing `generic/131` and `generic/257`

If strict POSIX-style compatibility becomes a product requirement later, revisit `atime` with an explicit performance budget and configuration story.
