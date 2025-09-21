//! The inner datastructure of a PrefixTrie that offers interior mutability.

use std::{
    cell::UnsafeCell,
    ops::{Index, IndexMut},
};

use crate::{to_right, Prefix};

#[derive(Clone)]
pub(crate) struct Node<P, T> {
    pub(crate) prefix: P,
    pub(crate) value: Option<T>,
    pub(crate) left: Option<usize>,
    pub(crate) right: Option<usize>,
}

impl<P, T> Node<P, T> {
    /// get the tuple of prefix and value.
    pub(crate) fn prefix_value(&self) -> Option<(&P, &T)> {
        self.value.as_ref().map(|v| (&self.prefix, v))
    }

    /// get the tuple of prefix and value.
    pub(crate) fn prefix_value_mut(&mut self) -> Option<(&P, &mut T)> {
        self.value.as_mut().map(|v| (&self.prefix, v))
    }
}

/// A table to the prefix-trie that offers interior mutability.
///
/// # Safety
/// Owning a mutable reference to the Table implies that you can safely get a mutable reference to
/// the inner data. If, however, you own an immutable reference, then you must guarantee that there
/// is no other reference to the Table that potentially accesses the same node mutably. This interior
/// mutability is only ever provided in `get_mut`.
pub(crate) struct Table<P, T>(UnsafeCell<Vec<Node<P, T>>>);

// Safety:
// - Sending a PrefixMap over thread boundary is fine. No-one besides us can have the raw pointer,
//   otherwise, the map would be borrowed.
// - Sending a reference of PrefixMap over thread boundaries (i.e., TrieView is Send) is safe,
//   because we ensure that the existence of a TrieView on a sub-tree implies the absence of a
//   TrieViewMut that overlaps with that sub-tree.
// - Sending a mutable reference of PrefixMap over thread boundaries (i.e., TrieView is Send) is
//   safe, because we ensure that the existence of a TrieViewMut on a sub-tree implies the absence
//   of any other TrieView or TrieViewMut that overlaps with that sub-tree.
// The same argument holds for Sync.
unsafe impl<P: Send, T: Send> Send for Table<P, T> {}
unsafe impl<P: Sync, T: Sync> Sync for Table<P, T> {}

impl<P, T> AsRef<Vec<Node<P, T>>> for Table<P, T> {
    fn as_ref(&self) -> &Vec<Node<P, T>> {
        // Safety: We own an immutable reference to the table.
        unsafe { self.0.get().as_ref().unwrap() }
    }
}

impl<P, T> AsMut<Vec<Node<P, T>>> for Table<P, T> {
    fn as_mut(&mut self) -> &mut Vec<Node<P, T>> {
        self.0.get_mut()
    }
}

impl<P, T> Index<usize> for Table<P, T> {
    type Output = Node<P, T>;

    fn index(&self, index: usize) -> &Self::Output {
        &self.as_ref()[index]
    }
}

impl<P, T> IndexMut<usize> for Table<P, T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.as_mut()[index]
    }
}

impl<P: Clone, T: Clone> Clone for Table<P, T> {
    fn clone(&self) -> Self {
        Self(UnsafeCell::new(self.as_ref().clone()))
    }
}

impl<P, T> Default for Table<P, T>
where
    P: Prefix,
{
    fn default() -> Self {
        Self(UnsafeCell::new(vec![Node {
            prefix: P::zero(),
            value: None,
            left: None,
            right: None,
        }]))
    }
}

pub(crate) enum Direction {
    /// The prefix is already reached.
    Reached,
    /// Enter the next index and search again.
    Enter { next: usize, right: bool },
    /// The node was not found.
    Missing,
}

pub(crate) enum DirectionForInsert<P> {
    /// The prefix is already reached.
    Reached,
    /// Enter the next index and search again.
    Enter { next: usize, right: bool },
    /// Insert a new child at the given position as a leaf.
    NewLeaf { right: bool },
    /// Insert a new child at the given position, moving all old children to be a child of the new
    /// prefix. `parent_right` tells where to insert the new node, while `child_right` tells where
    /// to insert the old child (on the right or the left of the new node).
    NewChild { right: bool, child_right: bool },
    /// Insert a new branch at the parent with the given prefix. `parent_right` tells where to
    /// insert the branch, while `prefix_right` tells where to insert the new node at the
    /// branch. The old child of the parent should be inserted at `!prefix_right` of the branch.
    NewBranch {
        branch_prefix: P,
        right: bool,
        prefix_right: bool,
    },
}

impl<P, T> Table<P, T> {
    pub(crate) fn into_inner(self) -> Vec<Node<P, T>> {
        self.0.into_inner()
    }

    /// *Safety*: You must ensure for the lifetime of 'a, that you will never construct a second
    /// reference to that node (neither mutable nor immutable).
    #[allow(clippy::mut_from_ref)]
    pub(crate) unsafe fn get_mut(&self, idx: usize) -> &mut Node<P, T> {
        // old implementation that caused issues with Miri:
        // unsafe { &mut self.0.get().as_mut().unwrap()[idx] }

        // new implementation based on manually offsetting the pointers:
        unsafe {
            // do the bounds check
            let len = self.0.get().as_ref().unwrap().len();
            if idx >= len {
                panic!("index out of bounds: the len is {len} but the index is {idx}");
            }
            let ptr_to_slice = self.0.get().as_ref().unwrap().as_ptr();
            let ptr_to_elem = ptr_to_slice.add(idx);
            (ptr_to_elem as *mut Node<P, T>).as_mut().unwrap()
        }
    }
}

impl<P: Prefix, T> Table<P, T> {
    /// Get the child of a node, either to the left or the right
    #[inline(always)]
    pub(crate) fn get_child(&self, idx: usize, right: bool) -> Option<usize> {
        if right {
            self[idx].right
        } else {
            self[idx].left
        }
    }

    /// set the child of a node (either to the left or the right), and return the index of the old child.
    #[inline(always)]
    pub(crate) fn set_child(&mut self, idx: usize, child: usize, right: bool) -> Option<usize> {
        if right {
            self[idx].right.replace(child)
        } else {
            self[idx].left.replace(child)
        }
    }

    /// remove a child from a node (just the reference).
    #[inline(always)]
    pub(crate) fn clear_child(&mut self, idx: usize, right: bool) -> Option<usize> {
        if right {
            self[idx].right.take()
        } else {
            self[idx].left.take()
        }
    }

    /// Get the directions from some node `idx` to get to `prefix`.
    #[inline(always)]
    pub(crate) fn get_direction(&self, cur: usize, prefix: &P) -> Direction {
        let cur_p = &self[cur].prefix;
        if cur_p.eq(prefix) {
            Direction::Reached
        } else {
            let right = to_right(cur_p, prefix);
            match self.get_child(cur, right) {
                Some(child) if self[child].prefix.contains(prefix) => {
                    Direction::Enter { next: child, right }
                }
                _ => Direction::Missing,
            }
        }
    }

    /// Get the directions from some node `idx` to get to `prefix`.
    #[inline(always)]
    pub(crate) fn get_direction_for_insert(&self, cur: usize, prefix: &P) -> DirectionForInsert<P> {
        let cur_p = &self[cur].prefix;
        if cur_p.eq(prefix) {
            DirectionForInsert::Reached
        } else {
            let right = to_right(cur_p, prefix);
            if let Some(child) = self.get_child(cur, right) {
                let child_p = &self[child].prefix;
                if child_p.contains(prefix) {
                    DirectionForInsert::Enter { next: child, right }
                } else if prefix.contains(child_p) {
                    DirectionForInsert::NewChild {
                        right,
                        child_right: to_right(prefix, child_p),
                    }
                } else {
                    let branch_prefix = prefix.longest_common_prefix(child_p);
                    let prefix_right = to_right(&branch_prefix, prefix);
                    DirectionForInsert::NewBranch {
                        branch_prefix,
                        right,
                        prefix_right,
                    }
                }
            } else {
                DirectionForInsert::NewLeaf { right }
            }
        }
    }
}
