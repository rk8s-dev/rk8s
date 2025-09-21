use criterion::{criterion_group, criterion_main, Criterion};
use ip_network_table_deps_treebitmap::IpLookupTable;
use ipnet::Ipv4Net;
use prefix_trie::*;
use rand::prelude::*;
use std::collections::HashSet;
use std::net::Ipv4Addr;

const ITERS: usize = 100_000;
const NUM_SPARSE_ADDR: usize = 20;

enum Insn {
    Insert(Ipv4Addr, u8, u32),
    Remove(Ipv4Addr, u8),
    ExactMatch(Ipv4Addr, u8),
    LongestPrefixMatch(Ipv4Addr, u8),
}

fn min_prefix_len(addr: u32) -> u8 {
    let mut bit: u32 = 0x00000001;
    let mut len: u8 = 32;
    while len > 0 && bit & addr == 0 {
        len = len.saturating_sub(1);
        (bit, _) = bit.overflowing_shl(1);
    }
    len
}

fn random_addr(rng: &mut ThreadRng) -> (Ipv4Addr, u8) {
    let addr: u32 = rng.gen::<u32>();
    let max_len = 32;
    let min_len = min_prefix_len(addr);
    let len = rng.gen_range(min_len..=max_len);
    (addr.into(), len)
}

fn generate_random_mods_dense() -> (Vec<Insn>, HashSet<(Ipv4Addr, u8)>) {
    let mut rng = thread_rng();
    let mut result = Vec::new();

    let mut addresses = HashSet::new();

    for _ in 0..ITERS {
        if addresses.is_empty() || rng.gen_bool(0.8) {
            let (addr, len) = random_addr(&mut rng);
            let val = rng.gen::<u32>();
            result.push(Insn::Insert(addr, len, val));
            addresses.insert((addr, len));
        } else {
            let (addr, len) = addresses
                .iter()
                .choose(&mut rng)
                .map(|(addr, len)| (*addr, *len))
                .unwrap();
            addresses.remove(&(addr, len));
            result.push(Insn::Remove(addr, len));
        }
    }
    (result, addresses)
}

fn generate_random_lookups_dense(addresses: &HashSet<(Ipv4Addr, u8)>) -> Vec<Insn> {
    let mut rng = thread_rng();
    let mut result = Vec::new();

    for _ in 0..ITERS {
        if rng.gen_bool(0.5) {
            let (addr, len) = if addresses.is_empty() || rng.gen_bool(0.5) {
                random_addr(&mut rng)
            } else {
                addresses
                    .iter()
                    .choose(&mut rng)
                    .map(|(addr, len)| (*addr, *len))
                    .unwrap()
            };
            result.push(Insn::ExactMatch(addr, len));
        } else {
            let (addr, len) = random_addr(&mut rng);
            result.push(Insn::LongestPrefixMatch(addr, len));
        }
    }
    result
}

fn sparse_addresses() -> Vec<(Ipv4Addr, u8)> {
    let mut rng = thread_rng();
    (0..NUM_SPARSE_ADDR)
        .map(|_| random_addr(&mut rng))
        .collect()
}

fn generate_random_mods_sparse(addresses: &[(Ipv4Addr, u8)]) -> Vec<Insn> {
    let mut rng = thread_rng();
    (0..ITERS)
        .map(|_| {
            let (addr, len) = addresses.iter().choose(&mut rng).unwrap();
            if rng.gen_bool(0.7) {
                let val = rng.gen::<u32>();
                Insn::Insert(*addr, *len, val)
            } else {
                Insn::Remove(*addr, *len)
            }
        })
        .collect()
}

fn generate_random_lookups_sparse(addresses: &[(Ipv4Addr, u8)]) -> Vec<Insn> {
    let mut rng = thread_rng();
    (0..ITERS)
        .map(|_| {
            let (addr, len) = addresses.iter().choose(&mut rng).unwrap();
            if rng.gen_bool(0.5) {
                Insn::ExactMatch(*addr, *len)
            } else {
                Insn::LongestPrefixMatch(*addr, *len)
            }
        })
        .collect()
}

fn execute_prefix_map(map: &mut PrefixMap<Ipv4Net, u32>, insns: &Vec<Insn>) {
    for insn in insns {
        criterion::black_box(match insn {
            Insn::Insert(addr, len, val) => map.insert(Ipv4Net::new(*addr, *len).unwrap(), *val),
            Insn::Remove(addr, len) => map.remove(&Ipv4Net::new(*addr, *len).unwrap()),
            Insn::ExactMatch(addr, len) => map.get(&Ipv4Net::new(*addr, *len).unwrap()).copied(),
            Insn::LongestPrefixMatch(addr, len) => map
                .get_lpm(&Ipv4Net::new(*addr, *len).unwrap())
                .map(|(_, x)| *x),
        });
    }
}

fn lookup_prefix_map(map: &PrefixMap<Ipv4Net, u32>, insns: &Vec<Insn>) {
    for insn in insns {
        criterion::black_box(match insn {
            Insn::Insert(_, _, _) => unreachable!(),
            Insn::Remove(_, _) => unreachable!(),
            Insn::ExactMatch(addr, len) => map.get(&Ipv4Net::new(*addr, *len).unwrap()).copied(),
            Insn::LongestPrefixMatch(addr, len) => map
                .get_lpm(&Ipv4Net::new(*addr, *len).unwrap())
                .map(|(_, x)| *x),
        });
    }
}

fn execute_treebitmap(map: &mut IpLookupTable<Ipv4Addr, u32>, insns: &Vec<Insn>) {
    for insn in insns {
        criterion::black_box(match insn {
            Insn::Insert(addr, len, val) => map.insert(*addr, *len as u32, *val),
            Insn::Remove(addr, len) => map.remove(*addr, *len as u32),
            Insn::ExactMatch(addr, len) => map.exact_match(*addr, *len as u32).copied(),
            Insn::LongestPrefixMatch(addr, _) => {
                let mut octets = addr.octets();
                octets[3] += 1;
                let addr = octets.into();
                map.longest_match(addr).map(|(_, _, x)| *x)
            }
        });
    }
}

fn lookup_treebitmap(map: &IpLookupTable<Ipv4Addr, u32>, insns: &Vec<Insn>) {
    for insn in insns {
        criterion::black_box(match insn {
            Insn::Insert(_, _, _) => unreachable!(),
            Insn::Remove(_, _) => unreachable!(),
            Insn::ExactMatch(addr, len) => map.exact_match(*addr, *len as u32).copied(),
            Insn::LongestPrefixMatch(addr, _) => {
                let mut octets = addr.octets();
                octets[3] += 1;
                let addr = octets.into();
                map.longest_match(addr).map(|(_, _, x)| *x)
            }
        });
    }
}

pub fn dense_mods(c: &mut Criterion) {
    let mut group = c.benchmark_group("dense modification");

    let (insn, _) = generate_random_mods_dense();

    group.bench_function("PrefixMap", |b| {
        b.iter(|| {
            let mut map = PrefixMap::new();
            execute_prefix_map(&mut map, &insn);
        })
    });
    group.bench_function("TreeBitMap dense", |b| {
        b.iter(|| {
            let mut map = IpLookupTable::new();
            execute_treebitmap(&mut map, &insn);
        })
    });

    group.finish();
}

pub fn dense_lookup(c: &mut Criterion) {
    let (mods, addrs) = generate_random_mods_dense();
    let lookups = generate_random_lookups_dense(&addrs);

    let mut prefix_map = PrefixMap::new();
    let mut treebitmap = IpLookupTable::new();
    execute_prefix_map(&mut prefix_map, &mods);
    execute_treebitmap(&mut treebitmap, &mods);

    let mut group = c.benchmark_group("dense lookups");

    group.bench_function("PrefixMap", |b| {
        b.iter(|| {
            lookup_prefix_map(&prefix_map, &lookups);
        })
    });
    group.bench_function("TreeBitMap", |b| {
        b.iter(|| {
            lookup_treebitmap(&treebitmap, &lookups);
        })
    });

    group.finish();
}

pub fn sparse_mods(c: &mut Criterion) {
    let mut group = c.benchmark_group("sparse modification");

    let addrs = sparse_addresses();
    let insn = generate_random_mods_sparse(&addrs);

    group.bench_function("PrefixMap", |b| {
        b.iter(|| {
            let mut map = PrefixMap::new();
            execute_prefix_map(&mut map, &insn);
        })
    });
    group.bench_function("TreeBitMap sparse", |b| {
        b.iter(|| {
            let mut map = IpLookupTable::new();
            execute_treebitmap(&mut map, &insn);
        })
    });

    group.finish();
}

pub fn sparse_lookup(c: &mut Criterion) {
    let mut group = c.benchmark_group("sparse lookups");

    let addrs = sparse_addresses();
    let mods = generate_random_mods_sparse(&addrs);
    let lookups = generate_random_lookups_sparse(&addrs);

    let mut prefix_map = PrefixMap::new();
    let mut treebitmap = IpLookupTable::new();
    execute_prefix_map(&mut prefix_map, &mods);
    execute_treebitmap(&mut treebitmap, &mods);

    group.bench_function("PrefixMap", |b| {
        b.iter(|| {
            lookup_prefix_map(&prefix_map, &lookups);
        })
    });
    group.bench_function("TreeBitMap", |b| {
        b.iter(|| {
            lookup_treebitmap(&treebitmap, &lookups);
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    dense_lookup,
    dense_mods,
    sparse_lookup,
    sparse_mods
);
criterion_main!(benches);
