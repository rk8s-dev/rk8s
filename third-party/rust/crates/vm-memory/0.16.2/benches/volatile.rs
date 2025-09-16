// Copyright (C) 2020 Alibaba Cloud. All rights reserved.
//
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

pub use criterion::{black_box, Criterion};
use vm_memory::volatile_memory::VolatileMemory;
use vm_memory::VolatileSlice;

pub fn benchmark_for_volatile(c: &mut Criterion) {
    let mut a = [0xa5u8; 1024];
    let vslice = VolatileSlice::from(&mut a[..]);
    let v_ref8 = vslice.get_slice(0, vslice.len()).unwrap();
    let mut d8 = [0u8; 1024];

    // Check performance for read operations.
    c.bench_function("VolatileSlice::copy_to_u8", |b| {
        b.iter(|| v_ref8.copy_to(black_box(&mut d8[..])))
    });

    let v_ref16 = vslice.get_slice(0, vslice.len() / 2).unwrap();
    let mut d16 = [0u16; 512];

    c.bench_function("VolatileSlice::copy_to_u16", |b| {
        b.iter(|| v_ref16.copy_to(black_box(&mut d16[..])))
    });
    benchmark_volatile_copy_to_volatile_slice(c);

    // Check performance for write operations.
    c.bench_function("VolatileSlice::copy_from_u8", |b| {
        b.iter(|| v_ref8.copy_from(black_box(&d8[..])))
    });
    c.bench_function("VolatileSlice::copy_from_u16", |b| {
        b.iter(|| v_ref16.copy_from(black_box(&d16[..])))
    });
}

fn benchmark_volatile_copy_to_volatile_slice(c: &mut Criterion) {
    let mut a = [0xa5u8; 10240];
    let vslice = VolatileSlice::from(&mut a[..]);
    let a_slice = vslice.get_slice(0, vslice.len()).unwrap();
    let mut d = [0u8; 10240];
    let vslice2 = VolatileSlice::from(&mut d[..]);
    let d_slice = vslice2.get_slice(0, vslice2.len()).unwrap();

    c.bench_function("VolatileSlice::copy_to_volatile_slice", |b| {
        b.iter(|| black_box(a_slice).copy_to_volatile_slice(d_slice))
    });
}
