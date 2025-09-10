# Changelog

## Upcoming version

## \[v0.16.2\]

- \[[#328](https://github.com/rust-vmm/vm-memory/pull/328)\] Bump vmm-sys-util crate to version 0.14.0

## \[v0.16.1\]

### Added

- \[[#304](https://github.com/rust-vmm/vm-memory/pull/304)\] Implement ReadVolatile and WriteVolatile for TcpStream

## \[v0.16.0\]

### Added

- \[[#287](https://github.com/rust-vmm/vm-memory/pull/287)\] Support for RISC-V 64-bit platform.
- \[[#299](https://github.com/rust-vmm/vm-memory/pull/299)\] atomic_bitmap: support enlarging the bitmap.

### Changed

- \[[#278](https://github.com/rust-vmm/vm-memory/pull/278) Remove `GuestMemoryIterator` trait,
  and instead have GuestMemory::iter() return `impl Iterator`.

## \[v0.15.0\]

### Added

- \[[#270](https://github.com/rust-vmm/vm-memory/pull/270)\] atomic_bitmap: add capability to reset bits range
- \[[#285](https://github.com/rust-vmm/vm-memory/pull/285)\] Annotated modules in lib.rs to indicate their feature
  dependencies such that it is reflected in the docs, enhancing documentation clarity for users.

### Changed

- \[[#275](https://github.com/rust-vmm/vm-memory/pull/275)\] Fail builds on non 64-bit platforms.

### Fixed

- \[[#279](https://github.com/rust-vmm/vm-memory/pull/279)\] Remove restriction from `read_volatile_from` and `write_volatile_into`
  that made it copy data it chunks of 4096.

### Removed

### Deprecated

## \[v0.14.0\]

### Added

- \[[#266](https://github.com/rust-vmm/vm-memory/pull/266)\] Derive `Debug` for several
  types that were missing it.

### Changed

- \[[#274](https://github.com/rust-vmm/vm-memory/pull/274)\] Drop `Default` as requirement for `ByteValued`.

## \[v0.13.1\]

### Added

- \[[#256](https://github.com/rust-vmm/vm-memory/pull/256)\] Implement `WriteVolatile`
  for `std::io::Stdout`.
- \[[#256](https://github.com/rust-vmm/vm-memory/pull/256)\] Implement `WriteVolatile`
  for `std::vec::Vec`.
- \[[#256](https://github.com/rust-vmm/vm-memory/pull/256)\] Implement `WriteVolatile`
  for `Cursor<&mut [u8]>`.
- \[[#256](https://github.com/rust-vmm/vm-memory/pull/256)\] Implement `ReadVolatile`
  for `Cursor<T: AsRef[u8]>`.

## \[v0.13.0\]

### Added

- [\[#247\]](https://github.com/rust-vmm/vm-memory/pull/247) Add `ReadVolatile` and
  `WriteVolatile` traits which are equivalents of `Read`/`Write` with volatile
  access semantics.

### Changed

- [\[#247\]](https://github.com/rust-vmm/vm-memory/pull/247) Deprecate
  `Bytes::{read_from, read_exact_from, write_to, write_all_to}`. Instead use
  `ReadVolatile`/`WriteVolatile`, which do not incur the performance penalty
  of copying to hypervisor memory due to `Read`/`Write` being incompatible
  with volatile semantics (see also #217).

## \[v0.12.2\]

### Fixed

- [\[#251\]](https://github.com/rust-vmm/vm-memory/pull/251): Inserted checks
  that verify that the value returned by `VolatileMemory::get_slice` is of
  the correct length.

### Deprecated

- [\[#244\]](https://github.com/rust-vmm/vm-memory/pull/241) Deprecate volatile
  memory's `as_ptr()` interfaces. The new interfaces to be used instead are:
  `ptr_guard()` and `ptr_guard_mut()`.

## \[v0.12.1\]

### Fixed

- [\[#241\]](https://github.com/rust-vmm/vm-memory/pull/245) mmap_xen: Don't drop
  the FileOffset while in use #245

## \[v0.12.0\]

### Added

- [\[#241\]](https://github.com/rust-vmm/vm-memory/pull/241) Add Xen memory
  mapping support: Foreign and Grant. Add new API for accessing pointers to
  volatile slices, as `as_ptr()` can't be used with Xen's Grant mapping.
- [\[#237\]](https://github.com/rust-vmm/vm-memory/pull/237) Implement `ByteValued` for `i/u128`.

## \[v0.11.0\]

### Added

- [\[#216\]](https://github.com/rust-vmm/vm-memory/pull/216) Add `GuestRegionMmap::from_region`.

### Fixed

- [\[#217\]](https://github.com/rust-vmm/vm-memory/pull/217) Fix vm-memory internally
  taking rust-style slices to guest memory in ways that could potentially cause
  undefined behavior. Removes/deprecates various `as_slice`/`as_slice_mut` methods
  whose usage violated rust's aliasing rules, as well as an unsound
  `impl<'a> VolatileMemory for &'a mut [u8]`.

## \[v0.10.0\]

### Changed

- [\[#208\]](https://github.com/rust-vmm/vm-memory/issues/208) Updated
  vmm-sys-util dependency to v0.11.0
- [\[#203\]](https://github.com/rust-vmm/vm-memory/pull/203) Switched to Rust
  edition 2021.

## \[v0.9.0\]

### Fixed

- [\[#195\]](https://github.com/rust-vmm/vm-memory/issues/195):
  `mmap::check_file_offset` is doing the correct size validation for block and
  char devices as well.

### Changed

- [\[#198\]](https://github.com/rust-vmm/vm-memory/pull/198): atomic: enable 64
  bit atomics on ppc64le and s390x.
- [\[#200\]](https://github.com/rust-vmm/vm-memory/pull/200): docs: enable all
  features in `docs.rs`.
- [\[#199\]](https://github.com/rust-vmm/vm-memory/issues/199): Update the way
  the dependencies are pulled such that we don't end up with incompatible
  versions.

## \[v0.8.0\]

### Fixed

- [\[#190\]](https://github.com/rust-vmm/vm-memory/pull/190):
  `VolatileSlice::read/write` when input slice is empty.

## \[v0.7.0\]

### Changed

- [\[#176\]](https://github.com/rust-vmm/vm-memory/pull/176): Relax the trait
  bounds of `Bytes` auto impl for `T: GuestMemory`
- [\[#178\]](https://github.com/rust-vmm/vm-memory/pull/178):
  `MmapRegion::build_raw` no longer requires that the length of the region is a
  multiple of the page size.

## \[v0.6.0\]

### Added

- [\[#160\]](https://github.com/rust-vmm/vm-memory/pull/160): Add `ArcRef` and `AtomicBitmapArc` bitmap
  backend implementations.
- [\[#149\]](https://github.com/rust-vmm/vm-memory/issues/149): Implement builder for MmapRegion.
- [\[#140\]](https://github.com/rust-vmm/vm-memory/issues/140): Add dirty bitmap tracking abstractions.

### Deprecated

- [\[#133\]](https://github.com/rust-vmm/vm-memory/issues/8): Deprecate `GuestMemory::with_regions()`,
  `GuestMemory::with_regions_mut()`, `GuestMemory::map_and_fold()`.

## \[v0.5.0\]

### Added

- [\[#8\]](https://github.com/rust-vmm/vm-memory/issues/8): Add GuestMemory method to return an Iterator
- [\[#120\]](https://github.com/rust-vmm/vm-memory/pull/120): Add is_hugetlbfs() to GuestMemoryRegion
- [\[#126\]](https://github.com/rust-vmm/vm-memory/pull/126): Add VolatileSlice::split_at()
- [\[#128\]](https://github.com/rust-vmm/vm-memory/pull/128): Add VolatileSlice::subslice()

## \[v0.4.0\]

### Fixed

- [\[#100\]](https://github.com/rust-vmm/vm-memory/issues/100): Performance
  degradation after fixing [#95](https://github.com/rust-vmm/vm-memory/pull/95).
- [\[#122\]](https://github.com/rust-vmm/vm-memory/pull/122): atomic,
  Cargo.toml: Update for arc-swap 1.0.0.

## \[v0.3.0\]

### Added

- [\[#109\]](https://github.com/rust-vmm/vm-memory/pull/109): Added `build_raw` to
  `MmapRegion` which can be used to operate on externally created mappings.
- [\[#101\]](https://github.com/rust-vmm/vm-memory/pull/101): Added `check_range` for
  GuestMemory which could be used to validate a range of guest memory.
- [\[#115\]](https://github.com/rust-vmm/vm-memory/pull/115): Add methods for atomic
  access to `Bytes`.

### Fixed

- [\[#93\]](https://github.com/rust-vmm/vm-memory/issues/93): DoS issue when using
  virtio with rust-vmm/vm-memory.
- [\[#106\]](https://github.com/rust-vmm/vm-memory/issues/106): Asserts trigger
  on zero-length access.

### Removed

- `integer-atomics` is no longer a distinct feature of the crate.

## \[v0.2.0\]

### Added

- [\[#76\]](https://github.com/rust-vmm/vm-memory/issues/76): Added `get_slice` and
  `as_volatile_slice` to `GuestMemoryRegion`.
- [\[#82\]](https://github.com/rust-vmm/vm-memory/issues/82): Added `Clone` bound
  for `GuestAddressSpace::T`, the return value of `GuestAddressSpace::memory()`.
- [\[#88\]](https://github.com/rust-vmm/vm-memory/issues/88): Added `as_bytes` for
  `ByteValued` which can be used for reading into POD structures from
  raw bytes.

## \[v0.1.0\]

### Added

- Added traits for working with VM memory.
- Added a mmap based implemention for the Guest Memory.
