[![CI test](https://img.shields.io/github/actions/workflow/status/tiborschneider/prefix-trie/test.yml)](https://github.com/tiborschneider/prefix-trie/actions)
[![codecov](https://codecov.io/gh/tiborschneider/prefix-trie/branch/main/graph/badge.svg?token=EEJXNNURMW)](https://codecov.io/gh/tiborschneider/prefix-trie)
[![version](https://img.shields.io/crates/v/prefix-trie)](https://crates.io/crates/prefix-trie)
[![downloads](https://img.shields.io/crates/d/prefix-trie)](https://crates.io/crates/prefix-trie)
[![docs.rs](https://img.shields.io/docsrs/prefix-trie/latest)](https://docs.rs/prefix-trie/latest/prefix_trie/)
[![license](https://img.shields.io/crates/l/prefix-trie/0.2.2)](https://crates.io/crates/prefix-trie)
[![Crates.io MSRV](https://img.shields.io/crates/msrv/prefix-trie)](https://crates.io/crates/prefix-trie)

# Prefix-Trie

This crate provides a simple prefix tree for IP prefixes. Any lookup performs longest-prefix
match. This crate supports both IPv4 and IPv6 (from either [ipnet](https://docs.rs/ipnet/2.10.0)
or [ipnetwork](https://crates.io/crates/ipnetwork) or [cidr](https://crates.io/crates/cidr)).
It also  supports any tuple `(R, u8)`, where `R` is any unsigned primitive integer (`u8`, `u16`,
`u32`, `u64`, `u128`, or `usize`).

## Comparison with related projects

[`ip_network_table-deps-treebitmap`](https://crates.io/crates/ip_network_table-deps-treebitmap)
provides an IP lookup table, similar to `PrefixMap`.

The following compares the two approaches in the case of *dense* or *sparse* maps. Each test case
performs 100'000 modifications or lookups. However, the dense cases randomly pick any IPv4
address, while the sparse case only picks 20 different IPv4 addresses. See `benches/benchmark.rs`
for more details.

| Operation       | Mode   | `PrefixMap` | `treebitmap` | factor |
|-----------------|--------|-------------|--------------|--------|
| Insert & Remove | dense  | **31.78ms** | 47.52ms      | ~1.5x  |
| Lookup          | dense  | 32.36ms     | **8.409ms**  | ~0.25x |
| Insert & Remove | sparse | **6.645ms** | 7.329ms      | ~1.1x  |
| Lookup          | sparse | **8.394ms** | 12.30ms      | ~1.5x  |

In addition, `prefix-trie` includes a `PrefixSet` analogous to `std::collections::HashSet`,
including union, intersection and difference operations that are implemented as simultaneous
tree traversals. Further, `prefix-trie` has an interface similar to `std::collections`, and
offers a general longest-prefix match that is not limited to individual addresses. Finally,
`prefix-trie` allows you to (mutably) borrow a sub-trie using views.

## Description of the Tree

The tree is structured as follows: Each node consists of a prefix, a container for a potential
value (`Option`), and two optional children. Adding a new child, or traversing into the tree is
done as follows: we look at the most significant bit that is **not** part of the prefix
itself. If it is not set, then we take the left branch, and otherwise, we take the right one.

# Traversals

Any iteration over all elements in the tree is implemented as a graph traversal that will yield
elements in lexicographic order.

The library offers set operations of different maps or sets. We implement a union, intersection,
difference, and covering_difference. These iterators are implemented using simultaneous tree
traversals. They will yield elements in lexicographic order. Whenever appropriate, the yielded
items will also include the longest prefix match.

# `TrieView` and `TrieViewMut`

You can create a view of a (sub)-trie. Such a view has an any node as its root. Any operations
on that view will only traverse that node and all its children. You can iterate over all
children, search in that sub-trie, and perform set operations (union, intersection, difference,
or the covering difference) on them.

A view can point to one of three possible nodes:
- A node in the tree that is actually present in the map,
- A branching node that does not exist in the map, but is needed for the tree structure (or that
  was deleted using the function `remove_keep_tree`)
- A virtual node that does not exist as a node in the tree. This is only the case if you call
  `PrefixView::find` or `AsView::view_at` with a node that is not present in the tree, but
  that contains elements present in the tree. Virtual nodes are treated as if they are actually
  present in the tree as branching nodes.

# Operations on the tree

There are several operations one can do on the tree. Regular inserts are handled using the
`Entry` structure. An `Entry` is a pointer to a location in the tree to either insert a value or
modify an existing one. Removals however are different.

The following are the computational complexities of the functions, where `n` is the number of
elements in the tree.

| Operation                                  | Complexity |
|--------------------------------------------|------------|
| `entry`, `insert`                          | `O(log n)` |
| `remove`, `remove_keep_tree`               | `O(log n)` |
| `remove_children` (calling `drop` on `T`)  | `O(n)`     |
| `get`, `get_lpm`, `get_mut`                | `O(log n)` |
| `retain`                                   | `O(n)`     |
| `clear` (calling `drop` on `T`)            | `O(n)`     |
| Operations on [`map::Entry`]               | `O(1)`     |
| `len` and `is_empty`                       | `O(1)`     |
| `union`, `intersection`, `difference`, ... | `O(n)`     |

There are three kinds of removals you! can do:

- `PrefixMap::remove` will remove an entry from the tree and modify the tree structure as if
  the value was never inserted before. `PrefixMap::remove` will always exactly revert the
  operation of `PrefixMap::insert`. When only calling this function to remove elements, you
  are guaranteed that the tree structure is indistinguishable to a different tree where you
  only inserted elements.
- `PrefixMap::remove_children` will remove all entries that are contained within the given
  prefix. This operation will search for the node with the shortest prefix length that is
  contained within the given prefix and remove it, including all of its children.
- `PrefixMap::remove_keep_tree` will not change anything in the tree structure. It will only
  remove a value from a node. As soon as you call `remove_keep_tree` once on a tree structure,
  the tree will no longer be optimal.

## TODO

Migrate to a TreeBitMap, described by [W. Eatherton, Z. Dittia, G. Varghes](https://doi.org/10.1145/997150.997160).
