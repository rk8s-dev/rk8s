# vm-memory

[![crates.io](https://img.shields.io/crates/v/vm-memory)](https://crates.io/crates/vm-memory)
[![docs.rs](https://img.shields.io/docsrs/vm-memory)](https://docs.rs/vm-memory/)

## Design

In a typical Virtual Machine Monitor (VMM) there are several components, such
as boot loader, virtual device drivers, virtio backend drivers and vhost
drivers, that need to access the VM physical memory. The `vm-memory` crate
provides a set of traits to decouple VM memory consumers from VM memory
providers. Based on these traits, VM memory consumers can access the physical
memory of the VM without knowing the implementation details of the VM memory
provider. Thus VMM components based on these traits can be shared and reused by
multiple virtualization solutions.

The detailed design of the `vm-memory` crate can be found [here](DESIGN.md).

### Platform Support

- Arch: x86_64, ARM64, RISCV64
- OS: Linux/Unix/Windows

### Xen support

Supporting Xen requires special handling while mapping the guest memory and
hence a separate feature is provided in the crate: `xen`. Mapping the guest
memory for Xen requires an `ioctl()` to be issued along with `mmap()` for the
memory area. The arguments for the `ioctl()` are received via the `vhost-user`
protocol's memory region area.

Xen allows two different mapping models: `Foreign` and `Grant`.

In `Foreign` mapping model, the entire guest address space is mapped at once, in
advance. In `Grant` mapping model, the memory for few regions, like those
representing the virtqueues, is mapped in advance. The rest of the memory
regions are mapped (partially) only while accessing the buffers and the same is
immediately deallocated after the buffer is accessed. Hence, special handling
for the same in `VolatileMemory.rs`.

In order to still support standard Unix memory regions, for special regions and
testing, the Xen specific implementation here allows a third mapping type:
`MmapXenFlags::UNIX`. This performs standard Unix memory mapping and the same is
used for all tests in this crate.

It was decided by the `rust-vmm` maintainers to keep the interface simple and
build the crate for either standard Unix memory mapping or Xen, and not both.

Xen is only supported for Unix platforms.

## Usage

Add `vm-memory` as a dependency in `Cargo.toml`

```toml
[dependencies]
vm-memory = "*"
```

Then add `extern crate vm-memory;` to your crate root.

## Examples

- Creating a VM physical memory object in hypervisor specific ways using the
  `GuestMemoryMmap` implementation of the `GuestMemory` trait:

```rust
fn provide_mem_to_virt_dev() {
    let gm = GuestMemoryMmap::from_ranges(&[
        (GuestAddress(0), 0x1000),
        (GuestAddress(0x1000), 0x1000)
    ]).unwrap();
    virt_device_io(&gm);
}
```

- Consumers accessing the VM's physical memory:

```rust
fn virt_device_io<T: GuestMemory>(mem: &T) {
    let sample_buf = &[1, 2, 3, 4, 5];
    assert_eq!(mem.write(sample_buf, GuestAddress(0xffc)).unwrap(), 5);
    let buf = &mut [0u8; 5];
    assert_eq!(mem.read(buf, GuestAddress(0xffc)).unwrap(), 5);
    assert_eq!(buf, sample_buf);
}
```

## License

This project is licensed under either of

- [Apache License](http://www.apache.org/licenses/LICENSE-2.0), Version 2.0
- [BSD-3-Clause License](https://opensource.org/licenses/BSD-3-Clause)
