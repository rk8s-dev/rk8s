//! Module that contains the implementation for the iterators

use map::Table;

use crate::*;

use super::Node;

/// An iterator over all entries of a [`PrefixMap`] in lexicographic order.
#[derive(Clone)]
pub struct Iter<'a, P, T> {
    table: Option<&'a Table<P, T>>,
    nodes: Vec<usize>,
}

impl<P, T> Default for Iter<'_, P, T> {
    fn default() -> Self {
        Self {
            table: None,
            nodes: Vec::new(),
        }
    }
}

impl<'a, P, T> Iter<'a, P, T> {
    pub(crate) fn new(table: &'a Table<P, T>, nodes: Vec<usize>) -> Self {
        Self {
            table: Some(table),
            nodes,
        }
    }
}

impl<'a, P, T> Iterator for Iter<'a, P, T> {
    type Item = (&'a P, &'a T);

    fn next(&mut self) -> Option<(&'a P, &'a T)> {
        while let Some(cur) = self.nodes.pop() {
            let node = &self.table.as_ref()?[cur];
            if let Some(right) = node.right {
                self.nodes.push(right);
            }
            if let Some(left) = node.left {
                self.nodes.push(left);
            }
            if let Some(v) = &node.value {
                return Some((&node.prefix, v));
            }
        }
        None
    }
}

/// An iterator over all prefixes of a [`PrefixMap`] in lexicographic order.
#[derive(Clone, Default)]
pub struct Keys<'a, P, T> {
    pub(crate) inner: Iter<'a, P, T>,
}

impl<'a, P, T> Iterator for Keys<'a, P, T> {
    type Item = &'a P;

    fn next(&mut self) -> Option<&'a P> {
        self.inner.next().map(|(k, _)| k)
    }
}

/// An iterator over all values of a [`PrefixMap`] in lexicographic order of their associated
/// prefixes.
#[derive(Clone, Default)]
pub struct Values<'a, P, T> {
    pub(crate) inner: Iter<'a, P, T>,
}

impl<'a, P, T> Iterator for Values<'a, P, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<&'a T> {
        self.inner.next().map(|(_, v)| v)
    }
}

/// An iterator over all owned entries of a [`PrefixMap`] in lexicographic order.
#[derive(Clone)]
pub struct IntoIter<P, T> {
    table: Vec<Node<P, T>>,
    nodes: Vec<usize>,
}

impl<P: Prefix, T> Iterator for IntoIter<P, T> {
    type Item = (P, T);

    fn next(&mut self) -> Option<(P, T)> {
        while let Some(cur) = self.nodes.pop() {
            let node = &mut self.table[cur];
            if let Some(right) = node.right {
                self.nodes.push(right);
            }
            if let Some(left) = node.left {
                self.nodes.push(left);
            }
            if let Some(v) = node.value.take() {
                return Some((std::mem::replace(&mut node.prefix, P::zero()), v));
            }
        }
        None
    }
}

/// An iterator over all prefixes of a [`PrefixMap`] in lexicographic order.
#[derive(Clone)]
pub struct IntoKeys<P, T> {
    inner: IntoIter<P, T>,
}

impl<P: Prefix, T> Iterator for IntoKeys<P, T> {
    type Item = P;

    fn next(&mut self) -> Option<P> {
        self.inner.next().map(|(k, _)| k)
    }
}

/// An iterator over all values of a [`PrefixMap`] in lexicographic order of their associated
/// prefix.
#[derive(Clone)]
pub struct IntoValues<P, T> {
    inner: IntoIter<P, T>,
}

impl<P: Prefix, T> Iterator for IntoValues<P, T> {
    type Item = T;

    fn next(&mut self) -> Option<T> {
        self.inner.next().map(|(_, v)| v)
    }
}

impl<P: Prefix, T> IntoIterator for PrefixMap<P, T> {
    type Item = (P, T);

    type IntoIter = IntoIter<P, T>;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter {
            table: self.table.into_inner(),
            nodes: vec![0],
        }
    }
}

impl<'a, P, T> IntoIterator for &'a PrefixMap<P, T> {
    type Item = (&'a P, &'a T);

    type IntoIter = Iter<'a, P, T>;

    fn into_iter(self) -> Self::IntoIter {
        // Safety: we own an immutable reference, and `Iter` will only ever read the table.
        Iter::new(&self.table, vec![0])
    }
}

/// A mutable iterator over a [`PrefixMap`]. This iterator yields elements in lexicographic order of
/// their associated prefix.
pub struct IterMut<'a, P, T> {
    table: Option<&'a Table<P, T>>,
    nodes: Vec<usize>,
}

impl<P, T> Default for IterMut<'_, P, T> {
    fn default() -> Self {
        Self {
            table: None,
            nodes: Vec::new(),
        }
    }
}

impl<'a, P, T> IterMut<'a, P, T> {
    /// # Safety
    /// - First, you must ensure that 'a is tied to a mutable reference of the original table.
    /// - Second, you are allowed to create mutiple such `IterMut`s, as long as none of the root
    ///   nodes is the parent of another root node (of any of the iterators). This also applies if
    ///   you only create a single iterator with multiple root nodes.
    ///
    /// The iterator will only ever access its roots or its children.
    pub(crate) unsafe fn new(table: &'a Table<P, T>, nodes: Vec<usize>) -> Self {
        Self {
            table: Some(table),
            nodes,
        }
    }
}

impl<'a, P, T> Iterator for IterMut<'a, P, T> {
    type Item = (&'a P, &'a mut T);

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(cur) = self.nodes.pop() {
            // Safety:
            // In the following, we assume that there are no two iterators that may reach the same
            // sub-tree (see the safety comment above).
            //
            // The iterator borrows from &'a mut PrefixMap, see `PrefixMap::iter_mut` where 'a is
            // linked to a mutable reference. Then, we must ensure that we only ever construct a
            // mutable reference to each element exactly once. We ensure this by the fact that we
            // iterate over a tree. Thus, each node is visited exactly once.
            let node: &'a mut Node<P, T> = unsafe { self.table.as_ref()?.get_mut(cur) };

            if let Some(right) = node.right {
                self.nodes.push(right);
            }
            if let Some(left) = node.left {
                self.nodes.push(left);
            }
            if node.value.is_some() {
                let v = node.value.as_mut().unwrap();
                return Some((&node.prefix, v));
            }
        }
        None
    }
}

/// A mutable iterator over values of [`PrefixMap`]. This iterator yields elements in lexicographic
/// order.
#[derive(Default)]
pub struct ValuesMut<'a, P, T> {
    // # Safety
    // You must ensure that there only ever exists one such iterator for each tree. You may create
    // multiple such iterators for the same tree if you start with distinct starting nodes! This
    // ensures that any one iteration will never yield elements of the other iterator.
    pub(crate) inner: IterMut<'a, P, T>,
}

impl<'a, P, T> Iterator for ValuesMut<'a, P, T> {
    type Item = &'a mut T;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(_, v)| v)
    }
}

impl<P, T> PrefixMap<P, T> {
    /// An iterator visiting all key-value pairs in lexicographic order. The iterator element type
    /// is `(&P, &T)`.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut pm: PrefixMap<ipnet::Ipv4Net, _> = PrefixMap::new();
    /// pm.insert("192.168.0.0/22".parse()?, 1);
    /// pm.insert("192.168.0.0/23".parse()?, 2);
    /// pm.insert("192.168.2.0/23".parse()?, 3);
    /// pm.insert("192.168.0.0/24".parse()?, 4);
    /// pm.insert("192.168.2.0/24".parse()?, 5);
    /// assert_eq!(
    ///     pm.iter().collect::<Vec<_>>(),
    ///     vec![
    ///         (&"192.168.0.0/22".parse()?, &1),
    ///         (&"192.168.0.0/23".parse()?, &2),
    ///         (&"192.168.0.0/24".parse()?, &4),
    ///         (&"192.168.2.0/23".parse()?, &3),
    ///         (&"192.168.2.0/24".parse()?, &5),
    ///     ]
    /// );
    /// # Ok(())
    /// # }
    /// # #[cfg(not(feature = "ipnet"))]
    /// # fn main() {}
    /// ```
    #[inline(always)]
    pub fn iter(&self) -> Iter<'_, P, T> {
        self.into_iter()
    }

    /// Get a mutable iterator over all key-value pairs. The order of this iterator is lexicographic.
    pub fn iter_mut(&mut self) -> IterMut<'_, P, T> {
        // Safety: We get the pointer to the table by and construct the `IterMut`. Its lifetime is
        // now tied to the mutable borrow of `self`, so we are allowed to access elements of that
        // table mutably.
        unsafe { IterMut::new(&self.table, vec![0]) }
    }

    /// An iterator visiting all keys in lexicographic order. The iterator element type is `&P`.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut pm: PrefixMap<ipnet::Ipv4Net, _> = PrefixMap::new();
    /// pm.insert("192.168.0.0/22".parse()?, 1);
    /// pm.insert("192.168.0.0/23".parse()?, 2);
    /// pm.insert("192.168.2.0/23".parse()?, 3);
    /// pm.insert("192.168.0.0/24".parse()?, 4);
    /// pm.insert("192.168.2.0/24".parse()?, 5);
    /// assert_eq!(
    ///     pm.keys().collect::<Vec<_>>(),
    ///     vec![
    ///         &"192.168.0.0/22".parse()?,
    ///         &"192.168.0.0/23".parse()?,
    ///         &"192.168.0.0/24".parse()?,
    ///         &"192.168.2.0/23".parse()?,
    ///         &"192.168.2.0/24".parse()?,
    ///     ]
    /// );
    /// # Ok(())
    /// # }
    /// # #[cfg(not(feature = "ipnet"))]
    /// # fn main() {}
    /// ```
    #[inline(always)]
    pub fn keys(&self) -> Keys<'_, P, T> {
        Keys { inner: self.iter() }
    }

    /// Creates a consuming iterator visiting all keys in lexicographic order. The iterator element
    /// type is `P`.
    #[inline(always)]
    pub fn into_keys(self) -> IntoKeys<P, T> {
        IntoKeys {
            inner: IntoIter {
                table: self.table.into_inner(),
                nodes: vec![0],
            },
        }
    }

    /// An iterator visiting all values in lexicographic order. The iterator element type is `&P`.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut pm: PrefixMap<ipnet::Ipv4Net, _> = PrefixMap::new();
    /// pm.insert("192.168.0.0/22".parse()?, 1);
    /// pm.insert("192.168.0.0/23".parse()?, 2);
    /// pm.insert("192.168.2.0/23".parse()?, 3);
    /// pm.insert("192.168.0.0/24".parse()?, 4);
    /// pm.insert("192.168.2.0/24".parse()?, 5);
    /// assert_eq!(pm.values().collect::<Vec<_>>(), vec![&1, &2, &4, &3, &5]);
    /// # Ok(())
    /// # }
    /// # #[cfg(not(feature = "ipnet"))]
    /// # fn main() {}
    /// ```
    #[inline(always)]
    pub fn values(&self) -> Values<'_, P, T> {
        Values { inner: self.iter() }
    }

    /// Creates a consuming iterator visiting all values in lexicographic order. The iterator
    /// element type is `P`.
    #[inline(always)]
    pub fn into_values(self) -> IntoValues<P, T> {
        IntoValues {
            inner: IntoIter {
                table: self.table.into_inner(),
                nodes: vec![0],
            },
        }
    }

    /// Get a mutable iterator over all values. The order of this iterator is lexicographic.
    pub fn values_mut(&mut self) -> ValuesMut<'_, P, T> {
        ValuesMut {
            inner: self.iter_mut(),
        }
    }
}

impl<P, T> PrefixMap<P, T>
where
    P: Prefix,
{
    /// Get an iterator over the node itself and all children. All elements returned have a prefix
    /// that is contained within `prefix` itself (or are the same). The iterator yields references
    /// to both keys and values, i.e., type `(&'a P, &'a T)`. The iterator yields elements in
    /// lexicographic order.
    ///
    /// **Note**: Consider using [`AsView::view_at`] as an alternative.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut pm: PrefixMap<ipnet::Ipv4Net, _> = PrefixMap::new();
    /// pm.insert("192.168.0.0/22".parse()?, 1);
    /// pm.insert("192.168.0.0/23".parse()?, 2);
    /// pm.insert("192.168.2.0/23".parse()?, 3);
    /// pm.insert("192.168.0.0/24".parse()?, 4);
    /// pm.insert("192.168.2.0/24".parse()?, 5);
    /// assert_eq!(
    ///     pm.children(&"192.168.0.0/23".parse()?).collect::<Vec<_>>(),
    ///     vec![
    ///         (&"192.168.0.0/23".parse()?, &2),
    ///         (&"192.168.0.0/24".parse()?, &4),
    ///     ]
    /// );
    /// # Ok(())
    /// # }
    /// # #[cfg(not(feature = "ipnet"))]
    /// # fn main() {}
    /// ```
    pub fn children<'a>(&'a self, prefix: &P) -> Iter<'a, P, T> {
        let nodes = lpm_children_iter_start(&self.table, prefix);
        Iter {
            table: Some(&self.table),
            nodes,
        }
    }

    /// Get an iterator of mutable references of the node itself and all its children. All elements
    /// returned have a prefix that is contained within `prefix` itself (or are the same). The
    /// iterator yields references to the keys, and mutable references to the values, i.e., type
    /// `(&'a P, &'a mut T)`. The iterator yields elements in lexicographic order.
    ///
    /// **Note**: Consider using [`AsViewMut::view_mut_at`] as an alternative.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut pm: PrefixMap<ipnet::Ipv4Net, _> = PrefixMap::new();
    /// pm.insert("192.168.0.0/22".parse()?, 1);
    /// pm.insert("192.168.0.0/23".parse()?, 2);
    /// pm.insert("192.168.0.0/24".parse()?, 3);
    /// pm.insert("192.168.2.0/23".parse()?, 4);
    /// pm.insert("192.168.2.0/24".parse()?, 5);
    /// pm.children_mut(&"192.168.0.0/23".parse()?).for_each(|(_, x)| *x *= 10);
    /// assert_eq!(
    ///     pm.into_iter().collect::<Vec<_>>(),
    ///     vec![
    ///         ("192.168.0.0/22".parse()?, 1),
    ///         ("192.168.0.0/23".parse()?, 20),
    ///         ("192.168.0.0/24".parse()?, 30),
    ///         ("192.168.2.0/23".parse()?, 4),
    ///         ("192.168.2.0/24".parse()?, 5),
    ///     ]
    /// );
    /// # Ok(())
    /// # }
    /// # #[cfg(not(feature = "ipnet"))]
    /// # fn main() {}
    /// ```
    pub fn children_mut<'a>(&'a mut self, prefix: &P) -> IterMut<'a, P, T> {
        let nodes = lpm_children_iter_start(&self.table, prefix);
        IterMut {
            table: Some(&self.table),
            nodes,
        }
    }

    /// Get an iterator over the node itself and all children with a value. All elements returned
    /// have a prefix that is contained within `prefix` itself (or are the same). This function will
    /// consume `self`, returning an iterator over all owned children.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut pm: PrefixMap<ipnet::Ipv4Net, _> = PrefixMap::new();
    /// pm.insert("192.168.0.0/22".parse()?, 1);
    /// pm.insert("192.168.0.0/23".parse()?, 2);
    /// pm.insert("192.168.2.0/23".parse()?, 3);
    /// pm.insert("192.168.0.0/24".parse()?, 4);
    /// pm.insert("192.168.2.0/24".parse()?, 5);
    /// assert_eq!(
    ///     pm.into_children(&"192.168.0.0/23".parse()?).collect::<Vec<_>>(),
    ///     vec![
    ///         ("192.168.0.0/23".parse()?, 2),
    ///         ("192.168.0.0/24".parse()?, 4),
    ///     ]
    /// );
    /// # Ok(())
    /// # }
    /// # #[cfg(not(feature = "ipnet"))]
    /// # fn main() {}
    /// ```
    pub fn into_children(self, prefix: &P) -> IntoIter<P, T> {
        let nodes = lpm_children_iter_start(&self.table, prefix);
        IntoIter {
            table: self.table.into_inner(),
            nodes,
        }
    }
}

fn lpm_children_iter_start<P: Prefix, T>(table: &Table<P, T>, prefix: &P) -> Vec<usize> {
    let mut idx = 0;
    let mut cur_p = &table[idx].prefix;

    loop {
        if cur_p.eq(prefix) {
            break vec![idx];
        }
        let right = to_right(cur_p, prefix);
        match table.get_child(idx, right) {
            Some(c) => {
                cur_p = &table[c].prefix;
                if cur_p.contains(prefix) {
                    // continue traversal
                    idx = c;
                } else if prefix.contains(cur_p) {
                    break vec![c];
                } else {
                    break vec![];
                }
            }
            None => break vec![],
        }
    }
}

impl<P, T> FromIterator<(P, T)> for PrefixMap<P, T>
where
    P: Prefix,
{
    fn from_iter<I: IntoIterator<Item = (P, T)>>(iter: I) -> Self {
        let mut map = Self::new();
        iter.into_iter().for_each(|(p, v)| {
            map.insert(p, v);
        });
        map
    }
}

/// An iterator that yields all items in a `PrefixMap` that covers a given prefix (including the
/// prefix itself if preseint). See [`PrefixMap::cover`] for how to create this iterator.
pub struct Cover<'a, 'p, P, T> {
    pub(super) table: &'a Table<P, T>,
    pub(super) idx: Option<usize>,
    pub(super) prefix: &'p P,
}

impl<'a, P, T> Iterator for Cover<'a, '_, P, T>
where
    P: Prefix,
{
    type Item = (&'a P, &'a T);

    fn next(&mut self) -> Option<Self::Item> {
        // check if self.idx is None. If so, then check if the first branch is present in the map
        if self.idx.is_none() {
            self.idx = Some(0);
            let entry = &self.table[0];
            if let Some(value) = entry.value.as_ref() {
                return Some((&entry.prefix, value));
            }
        }

        // if we reach here, then self.idx is not None!

        loop {
            let map::Direction::Enter { next, .. } =
                self.table.get_direction(self.idx.unwrap(), self.prefix)
            else {
                return None;
            };
            self.idx = Some(next);
            let entry = &self.table[next];
            if let Some(value) = entry.value.as_ref() {
                return Some((&entry.prefix, value));
            }
        }
    }
}

/// An iterator that yields all keys (prefixes) in a `PrefixMap` that covers a given prefix
/// (including the prefix itself if preseint). See [`PrefixMap::cover_keys`] for how to create this
/// iterator.
pub struct CoverKeys<'a, 'p, P, T>(pub(super) Cover<'a, 'p, P, T>);

impl<'a, P, T> Iterator for CoverKeys<'a, '_, P, T>
where
    P: Prefix,
{
    type Item = &'a P;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|(p, _)| p)
    }
}

/// An iterator that yields all values in a `PrefixMap` that covers a given prefix (including the
/// prefix itself if preseint). See [`PrefixMap::cover_values`] for how to create this iterator.
pub struct CoverValues<'a, 'p, P, T>(pub(super) Cover<'a, 'p, P, T>);

impl<'a, P, T> Iterator for CoverValues<'a, '_, P, T>
where
    P: Prefix,
{
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|(_, t)| t)
    }
}
