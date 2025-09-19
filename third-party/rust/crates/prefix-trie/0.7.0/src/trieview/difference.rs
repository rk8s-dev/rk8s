use crate::to_right;

use super::*;

/// An iterator over the difference of two [`TrieView`]s. See [`TrieView::difference`] for more
/// information and an example.
pub struct Difference<'a, P, L, R> {
    table_l: &'a Table<P, L>,
    table_r: &'a Table<P, R>,
    nodes: Vec<(DifferenceIndex, Option<(&'a P, &'a R)>)>,
}

/// An iterator over the difference of two [`TrieView`]s. See [`TrieViewMut::difference_mut`] for
/// more information and an example.
pub struct DifferenceMut<'a, P, L, R> {
    table_l: &'a Table<P, L>,
    table_r: &'a Table<P, R>,
    nodes: Vec<(DifferenceIndex, Option<(&'a P, &'a R)>)>,
}

impl<'a, P, L, R> DifferenceMut<'a, P, L, R> {
    /// Safety:
    /// 1. Table_l must come from a `TrieViewMut` and satisfy the conditions in `TrieViewMut::new`.
    /// 2. Table_l and table_r must be distinct. This is implicitly given by the rule above.
    unsafe fn new(
        table_l: &'a Table<P, L>,
        table_r: &'a Table<P, R>,
        nodes: Vec<(DifferenceIndex, Option<(&'a P, &'a R)>)>,
    ) -> Self {
        Self {
            table_l,
            table_r,
            nodes,
        }
    }
}

/// An iterator over the covering difference of two [`TrieView`]s. See
/// [`TrieView::covering_difference`] for more information and an example.
pub struct CoveringDifference<'a, P, L, R> {
    table_l: &'a Table<P, L>,
    table_r: &'a Table<P, R>,
    nodes: Vec<DifferenceIndex>,
}

/// An iterator over the covering difference of two [`TrieView`]s. See
/// [`TrieViewMut::covering_difference_mut`] for more information and an example.
pub struct CoveringDifferenceMut<'a, P, L, R> {
    table_l: &'a Table<P, L>,
    table_r: &'a Table<P, R>,
    nodes: Vec<DifferenceIndex>,
}

impl<'a, P, L, R> CoveringDifferenceMut<'a, P, L, R> {
    /// Safety:
    /// 1. Table_l must come from a `TrieViewMut` and satisfy the conditions in `TrieViewMut::new`.
    /// 2. Table_l and table_r must be distinct. This is implicitly given by the rule above.
    unsafe fn new(
        table_l: &'a Table<P, L>,
        table_r: &'a Table<P, R>,
        nodes: Vec<DifferenceIndex>,
    ) -> Self {
        Self {
            table_l,
            table_r,
            nodes,
        }
    }
}

pub(super) enum DifferenceIndex {
    Both(usize, usize),
    FirstL(usize, usize),
    FirstR(usize, usize),
    OnlyL(usize),
}

/// An item of the [`Difference`] iterator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DifferenceItem<'a, P, L, R> {
    /// The prefix that is present in `self` but not in `other`.
    pub prefix: &'a P,
    /// The value that is stored in `self`.
    pub value: &'a L,
    /// The longest-prefix-match that is present in `other`.
    pub right: Option<(&'a P, &'a R)>,
}

/// An item of the [`DifferenceMut`] iterator.
#[derive(Debug)]
pub struct DifferenceMutItem<'a, P, L, R> {
    /// The prefix that is present in `self` but not in `other`.
    pub prefix: &'a P,
    /// The value that is stored in `self`.
    pub value: &'a mut L,
    /// The longest-prefix-match that is present in `other`.
    pub right: Option<(&'a P, &'a R)>,
}

impl<'a, P, L> TrieView<'a, P, L>
where
    P: Prefix,
{
    /// Iterate over the all elements in `self` that are not present in `other`. Each item will
    /// return a reference to the prefix and value in `self`, as well as the longest prefix match of
    /// `other`.
    ///
    /// **Warning**: The iterator will only yield elements of the given views. If `other` points to
    /// a branching or a virtual node, then the longest prefix match returned may be `None`, even
    /// though it exists in the original map referenced by `other`.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # use prefix_trie::trieview::DifferenceItem;
    /// # #[cfg(feature = "ipnet")]
    /// macro_rules! net { ($x:literal) => {$x.parse::<ipnet::Ipv4Net>().unwrap()}; }
    ///
    /// # #[cfg(feature = "ipnet")]
    /// # {
    /// let mut map_a: PrefixMap<ipnet::Ipv4Net, usize> = PrefixMap::from_iter([
    ///     (net!("192.168.0.0/20"), 1),
    ///     (net!("192.168.0.0/22"), 2),
    ///     (net!("192.168.0.0/24"), 3),
    ///     (net!("192.168.2.0/23"), 4),
    /// ]);
    /// let mut map_b: PrefixMap<ipnet::Ipv4Net, &'static str> = PrefixMap::from_iter([
    ///     (net!("192.168.0.0/20"), "a"),
    ///     (net!("192.168.0.0/22"), "b"),
    ///     (net!("192.168.0.0/23"), "c"),
    ///     (net!("192.168.2.0/24"), "d"),
    /// ]);
    /// let sub_a = map_a.view_at(net!("192.168.0.0/22")).unwrap();
    /// let sub_b = map_b.view_at(net!("192.168.0.0/22")).unwrap();
    /// assert_eq!(
    ///     sub_a.difference(sub_b).collect::<Vec<_>>(),
    ///     vec![
    ///         DifferenceItem { prefix: &net!("192.168.0.0/24"), value: &3, right: Some((&net!("192.168.0.0/23"), &"c"))},
    ///         DifferenceItem { prefix: &net!("192.168.2.0/23"), value: &4, right: Some((&net!("192.168.0.0/22"), &"b"))},
    ///     ]
    /// );
    /// # }
    /// ```
    pub fn difference<R>(&self, other: impl AsView<'a, P, R>) -> Difference<'a, P, L, R> {
        let other = other.view();
        Difference {
            table_l: self.table,
            table_r: other.table,
            nodes: extend_lpm(
                other.table,
                other.table[other.loc.idx()].prefix_value(),
                next_indices(
                    self.table,
                    other.table,
                    Some(self.loc.idx()),
                    Some(other.loc.idx()),
                ),
            )
            .collect(),
        }
    }

    /// Iterate over all elements in `self` that are not not covered in `other`. In other words,
    /// only items of `self` are yielded for which there does not exist a prefix in `other` that
    /// covers it.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// macro_rules! net { ($x:literal) => {$x.parse::<ipnet::Ipv4Net>().unwrap()}; }
    ///
    /// # #[cfg(feature = "ipnet")]
    /// # {
    /// let mut map_a: PrefixMap<ipnet::Ipv4Net, usize> = PrefixMap::from_iter([
    ///     (net!("192.168.0.0/22"), 1),
    ///     (net!("192.168.0.0/24"), 2),
    ///     (net!("192.168.2.0/23"), 3),
    /// ]);
    /// let mut set_b: PrefixSet<ipnet::Ipv4Net> = PrefixSet::from_iter([
    ///     net!("192.168.0.0/23"),
    /// ]);
    /// assert_eq!(
    ///     map_a.view().covering_difference(&set_b).collect::<Vec<_>>(),
    ///     vec![(&net!("192.168.0.0/22"), &1), (&net!("192.168.2.0/23"), &3)],
    /// );
    /// # }
    /// ```
    pub fn covering_difference<R>(
        &self,
        other: impl AsView<'a, P, R>,
    ) -> CoveringDifference<'a, P, L, R> {
        let other = other.view();
        CoveringDifference {
            table_l: self.table,
            table_r: other.table,
            nodes: next_indices(
                self.table,
                other.table,
                Some(self.loc.idx()),
                Some(other.loc.idx()),
            ),
        }
    }
}

impl<P, L> TrieViewMut<'_, P, L>
where
    P: Prefix,
{
    /// Iterate over the all elements in `self` that are not present in `other`. Each item will
    /// return a reference to the prefix and a mutable reference to the value in `self`, as well as
    /// the longest prefix match of `other` (with an immutable reference).
    ///
    /// **Warning**: The iterator will only yield elements of the given views. If `other` points to
    /// a branching node, then the longest prefix match returned may be `None`, even though it
    /// exists in the original map referenced by `other`.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # use prefix_trie::trieview::DifferenceItem;
    /// # #[cfg(feature = "ipnet")]
    /// macro_rules! net { ($x:literal) => {$x.parse::<ipnet::Ipv4Net>().unwrap()}; }
    ///
    /// # #[cfg(feature = "ipnet")]
    /// # {
    /// let mut map_a: PrefixMap<ipnet::Ipv4Net, usize> = PrefixMap::from_iter([
    ///     (net!("192.168.0.0/20"), 1),
    ///     (net!("192.168.0.0/22"), 2),
    ///     (net!("192.168.0.0/24"), 3),
    ///     (net!("192.168.2.0/23"), 4),
    /// ]);
    /// let mut map_b: PrefixMap<ipnet::Ipv4Net, &'static str> = PrefixMap::from_iter([
    ///     (net!("192.168.0.0/20"), "a"),
    ///     (net!("192.168.0.0/22"), "b"),
    ///     (net!("192.168.0.0/23"), "c"),
    ///     (net!("192.168.2.0/24"), "d"),
    /// ]);
    ///
    /// let mut sub_a = map_a.view_mut_at(net!("192.168.0.0/22")).unwrap();
    /// let sub_b = map_b.view_at(net!("192.168.0.0/22")).unwrap();
    /// sub_a.difference_mut(sub_b).for_each(|x| *x.value += 10);
    ///
    /// assert_eq!(
    ///     map_a.into_iter().collect::<Vec<_>>(),
    ///     vec![
    ///         (net!("192.168.0.0/20"), 1),
    ///         (net!("192.168.0.0/22"), 2),
    ///         (net!("192.168.0.0/24"), 13),
    ///         (net!("192.168.2.0/23"), 14),
    ///     ]
    /// );
    /// # }
    /// ```
    pub fn difference_mut<'b, R>(
        &'b mut self,
        other: impl AsView<'b, P, R>,
    ) -> DifferenceMut<'b, P, L, R> {
        let other = other.view();
        let nodes = extend_lpm(
            other.table,
            other.table[other.loc.idx()].prefix_value(),
            next_indices(
                self.table,
                other.table,
                Some(self.loc.idx()),
                Some(other.loc.idx()),
            ),
        )
        .collect();
        // Safety: `self` comes from a TrieViewMut. Assuming it satisfies all conditions from
        // `TrieViewMut::new`, then `self.table` is the the only thing possibly referencing
        // its nodes.
        unsafe { DifferenceMut::new(self.table, other.table, nodes) }
    }

    /// Iterate over all elements in `self` that are not not covered in `other`. In other words,
    /// only items of `self` are yielded for which there does not exist a prefix in `other` that
    /// covers it. The iterator yields mutable references to the values.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// macro_rules! net { ($x:literal) => {$x.parse::<ipnet::Ipv4Net>().unwrap()}; }
    ///
    /// # #[cfg(feature = "ipnet")]
    /// # {
    /// let mut map_a: PrefixMap<ipnet::Ipv4Net, usize> = PrefixMap::from_iter([
    ///     (net!("192.168.0.0/22"), 1),
    ///     (net!("192.168.0.0/24"), 2),
    ///     (net!("192.168.2.0/23"), 3),
    /// ]);
    /// let mut set_b: PrefixSet<ipnet::Ipv4Net> = PrefixSet::from_iter([
    ///     net!("192.168.0.0/23"),
    /// ]);
    ///
    /// map_a.view_mut().covering_difference_mut(&set_b).for_each(|(_, v)| *v += 10);
    ///
    /// assert_eq!(
    ///     map_a.into_iter().collect::<Vec<_>>(),
    ///     vec![
    ///         (net!("192.168.0.0/22"), 11),
    ///         (net!("192.168.0.0/24"), 2),
    ///         (net!("192.168.2.0/23"), 13),
    ///     ],
    /// );
    /// # }
    /// ```
    pub fn covering_difference_mut<'b, R>(
        &'b mut self,
        other: impl AsView<'b, P, R>,
    ) -> CoveringDifferenceMut<'b, P, L, R> {
        let other = other.view();
        let nodes = next_indices(
            self.table,
            other.table,
            Some(self.loc.idx()),
            Some(other.loc.idx()),
        );

        // Safety: `self` comes from a TrieViewMut. Assuming it satisfies all conditions from
        // `TrieViewMut::new`, then `self.table` is the the only thing possibly referencing
        // its nodes.
        unsafe { CoveringDifferenceMut::new(self.table, other.table, nodes) }
    }
}

impl<'a, P: Prefix, L, R> Iterator for Difference<'a, P, L, R> {
    type Item = DifferenceItem<'a, P, L, R>;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some((cur, lpm_r)) = self.nodes.pop() {
            match cur {
                DifferenceIndex::Both(l, r) => {
                    let node_l = &self.table_l[l];
                    let node_r = &self.table_r[r];
                    self.extend(
                        next_indices(self.table_l, self.table_r, node_l.right, node_r.right),
                        lpm_r,
                    );
                    self.extend(
                        next_indices(self.table_l, self.table_r, node_l.left, node_r.left),
                        lpm_r,
                    );
                    if let Some(value) = node_l.value.as_ref() {
                        if node_r.value.is_none() {
                            return Some(DifferenceItem {
                                prefix: &node_l.prefix,
                                value,
                                right: lpm_r,
                            });
                        }
                    }
                }
                DifferenceIndex::FirstL(l, r) => {
                    let node_l = &self.table_l[l];
                    self.extend(
                        next_indices_first_a(
                            self.table_l,
                            self.table_r,
                            l,
                            node_l.left,
                            node_l.right,
                            r,
                        ),
                        lpm_r,
                    );
                    if let Some(value) = node_l.value.as_ref() {
                        return Some(DifferenceItem {
                            prefix: &node_l.prefix,
                            value,
                            right: lpm_r,
                        });
                    }
                }
                DifferenceIndex::FirstR(l, r) => {
                    let node_r = &self.table_r[r];
                    self.extend(
                        next_indices_first_b(
                            self.table_l,
                            self.table_r,
                            l,
                            r,
                            node_r.left,
                            node_r.right,
                        ),
                        lpm_r,
                    );
                }
                DifferenceIndex::OnlyL(l) => {
                    let node_l = &self.table_l[l];
                    if let Some(right) = node_l.right {
                        self.extend([DifferenceIndex::OnlyL(right)], lpm_r);
                    }
                    if let Some(left) = node_l.left {
                        self.extend([DifferenceIndex::OnlyL(left)], lpm_r);
                    }
                    if let Some(value) = node_l.value.as_ref() {
                        return Some(DifferenceItem {
                            prefix: &node_l.prefix,
                            value,
                            right: lpm_r,
                        });
                    }
                }
            }
        }
        None
    }
}

impl<'a, P: Prefix, L, R> Iterator for CoveringDifference<'a, P, L, R> {
    type Item = (&'a P, &'a L);

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(cur) = self.nodes.pop() {
            match cur {
                DifferenceIndex::Both(l, r) => {
                    let node_l = &self.table_l[l];
                    let node_r = &self.table_r[r];
                    // skip if r has a value (this all children must be ignored)
                    if node_r.value.is_some() {
                        continue;
                    }
                    self.nodes.extend(next_indices(
                        self.table_l,
                        self.table_r,
                        node_l.right,
                        node_r.right,
                    ));
                    self.nodes.extend(next_indices(
                        self.table_l,
                        self.table_r,
                        node_l.left,
                        node_r.left,
                    ));
                    if let Some(value) = node_l.value.as_ref() {
                        return Some((&node_l.prefix, value));
                    }
                }
                DifferenceIndex::FirstL(l, r) => {
                    let node_l = &self.table_l[l];
                    self.nodes.extend(next_indices_first_a(
                        self.table_l,
                        self.table_r,
                        l,
                        node_l.left,
                        node_l.right,
                        r,
                    ));
                    if let Some(value) = node_l.value.as_ref() {
                        return Some((&node_l.prefix, value));
                    }
                }
                DifferenceIndex::FirstR(l, r) => {
                    let node_r = &self.table_r[r];
                    // skip if r has a value (this all children must be ignored)
                    if node_r.value.is_some() {
                        continue;
                    }
                    self.nodes.extend(next_indices_first_b(
                        self.table_l,
                        self.table_r,
                        l,
                        r,
                        node_r.left,
                        node_r.right,
                    ));
                }
                DifferenceIndex::OnlyL(l) => {
                    let node_l = &self.table_l[l];
                    if let Some(right) = node_l.right {
                        self.nodes.extend([DifferenceIndex::OnlyL(right)]);
                    }
                    if let Some(left) = node_l.left {
                        self.nodes.extend([DifferenceIndex::OnlyL(left)]);
                    }
                    if let Some(value) = node_l.value.as_ref() {
                        return Some((&node_l.prefix, value));
                    }
                }
            }
        }
        None
    }
}

impl<'a, P: Prefix, L, R> Iterator for DifferenceMut<'a, P, L, R> {
    type Item = DifferenceMutItem<'a, P, L, R>;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some((cur, lpm_r)) = self.nodes.pop() {
            // safety: map is a tree. Every node is visited exactly once during the iteration
            // (self.nodes is not public). Therefore, each in each iteration of this loop (also
            // between multiple calls to `next`), the index `cur` is different to any of the earlier
            // iterations. It is therefore safe to extend the lifetime of the elements to 'a (which
            // is the lifetime for which `self` has an exclusive reference over the map).
            match cur {
                DifferenceIndex::Both(l, r) => {
                    let node_l = &self.table_l[l];
                    let node_r = &self.table_r[r];
                    self.extend(
                        next_indices(self.table_l, self.table_r, node_l.right, node_r.right),
                        lpm_r,
                    );
                    self.extend(
                        next_indices(self.table_l, self.table_r, node_l.left, node_r.left),
                        lpm_r,
                    );
                    let node_l = unsafe { self.table_l.get_mut(l) };
                    if let Some(value) = node_l.value.as_mut() {
                        if node_r.value.is_none() {
                            return Some(DifferenceMutItem {
                                prefix: &node_l.prefix,
                                value,
                                right: lpm_r,
                            });
                        }
                    }
                }
                DifferenceIndex::FirstL(l, r) => {
                    let node_l = &self.table_l[l];
                    self.extend(
                        next_indices_first_a(
                            self.table_l,
                            self.table_r,
                            l,
                            node_l.left,
                            node_l.right,
                            r,
                        ),
                        lpm_r,
                    );
                    let node_l = unsafe { self.table_l.get_mut(l) };
                    if let Some(value) = node_l.value.as_mut() {
                        return Some(DifferenceMutItem {
                            prefix: &node_l.prefix,
                            value,
                            right: lpm_r,
                        });
                    }
                }
                DifferenceIndex::FirstR(l, r) => {
                    let node_r = &self.table_r[r];
                    self.extend(
                        next_indices_first_b(
                            self.table_l,
                            self.table_r,
                            l,
                            r,
                            node_r.left,
                            node_r.right,
                        ),
                        lpm_r,
                    );
                }
                DifferenceIndex::OnlyL(l) => {
                    let node_l = unsafe { self.table_l.get_mut(l) };
                    if let Some(right) = node_l.right {
                        self.extend([DifferenceIndex::OnlyL(right)], lpm_r);
                    }
                    if let Some(left) = node_l.left {
                        self.extend([DifferenceIndex::OnlyL(left)], lpm_r);
                    }
                    if let Some(value) = node_l.value.as_mut() {
                        return Some(DifferenceMutItem {
                            prefix: &node_l.prefix,
                            value,
                            right: lpm_r,
                        });
                    }
                }
            }
        }
        None
    }
}

impl<'a, P: Prefix, L, R> Iterator for CoveringDifferenceMut<'a, P, L, R> {
    type Item = (&'a P, &'a mut L);

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(cur) = self.nodes.pop() {
            // safety: map is a tree. Every node is visited exactly once during the iteration
            // (self.nodes is not public). Therefore, each in each iteration of this loop (also
            // between multiple calls to `next`), the index `cur` is different to any of the earlier
            // iterations. It is therefore safe to extend the lifetime of the elements to 'a (which
            // is the lifetime for which `self` has an exclusive reference over the map).
            match cur {
                DifferenceIndex::Both(l, r) => {
                    let node_l = &self.table_l[l];
                    let node_r = &self.table_r[r];
                    // skip if r has a value (this all children must be ignored)
                    if node_r.value.is_some() {
                        continue;
                    }
                    self.nodes.extend(next_indices(
                        self.table_l,
                        self.table_r,
                        node_l.right,
                        node_r.right,
                    ));
                    self.nodes.extend(next_indices(
                        self.table_l,
                        self.table_r,
                        node_l.left,
                        node_r.left,
                    ));
                    let node_l = unsafe { self.table_l.get_mut(l) };
                    if let Some(value) = node_l.value.as_mut() {
                        return Some((&node_l.prefix, value));
                    }
                }
                DifferenceIndex::FirstL(l, r) => {
                    let node_l = &self.table_l[l];
                    self.nodes.extend(next_indices_first_a(
                        self.table_l,
                        self.table_r,
                        l,
                        node_l.left,
                        node_l.right,
                        r,
                    ));
                    let node_l = unsafe { self.table_l.get_mut(l) };
                    if let Some(value) = node_l.value.as_mut() {
                        return Some((&node_l.prefix, value));
                    }
                }
                DifferenceIndex::FirstR(l, r) => {
                    let node_r = &self.table_r[r];
                    // skip if r has a value (this all children must be ignored)
                    if node_r.value.is_some() {
                        continue;
                    }
                    self.nodes.extend(next_indices_first_b(
                        self.table_l,
                        self.table_r,
                        l,
                        r,
                        node_r.left,
                        node_r.right,
                    ));
                }
                DifferenceIndex::OnlyL(l) => {
                    let node_l = unsafe { self.table_l.get_mut(l) };
                    if let Some(right) = node_l.right {
                        self.nodes.extend([DifferenceIndex::OnlyL(right)]);
                    }
                    if let Some(left) = node_l.left {
                        self.nodes.extend([DifferenceIndex::OnlyL(left)]);
                    }
                    if let Some(value) = node_l.value.as_mut() {
                        return Some((&node_l.prefix, value));
                    }
                }
            }
        }
        None
    }
}

impl<'a, P: Prefix, L, R> Difference<'a, P, L, R> {
    fn extend(
        &mut self,
        indices: impl IntoIterator<Item = DifferenceIndex> + 'static,
        lpm_r: Option<(&'a P, &'a R)>,
    ) {
        self.nodes.extend(extend_lpm(self.table_r, lpm_r, indices));
    }
}

impl<'a, P: Prefix, L, R> DifferenceMut<'a, P, L, R> {
    fn extend(
        &mut self,
        indices: impl IntoIterator<Item = DifferenceIndex> + 'static,
        lpm_r: Option<(&'a P, &'a R)>,
    ) {
        self.nodes.extend(extend_lpm(self.table_r, lpm_r, indices));
    }
}

fn next_indices<'a, P: Prefix, L, R>(
    table_l: &'a Table<P, L>,
    table_r: &'a Table<P, R>,
    l: Option<usize>,
    r: Option<usize>,
) -> Vec<DifferenceIndex> {
    match (l, r) {
        (None, Some(_)) => vec![],
        (Some(l), None) => vec![DifferenceIndex::OnlyL(l)],
        (Some(l), Some(r)) => {
            let p_l = &table_l[l].prefix;
            let p_r = &table_r[r].prefix;
            if p_l.prefix_len() == p_r.prefix_len() {
                match p_l.mask().cmp(&p_r.mask()) {
                    std::cmp::Ordering::Equal => {
                        vec![DifferenceIndex::Both(l, r)]
                    }
                    _ => {
                        vec![DifferenceIndex::OnlyL(l)]
                    }
                }
            } else if p_l.contains(p_r) {
                vec![DifferenceIndex::FirstL(l, r)]
            } else if p_r.contains(p_l) {
                vec![DifferenceIndex::FirstR(l, r)]
            } else {
                vec![DifferenceIndex::OnlyL(l)]
            }
        }
        _ => vec![],
    }
}

fn next_indices_first_a<'a, P: Prefix, L, R>(
    table_l: &'a Table<P, L>,
    table_r: &'a Table<P, R>,
    l: usize,
    ll: Option<usize>,
    lr: Option<usize>,
    r: usize,
) -> Vec<DifferenceIndex> {
    match (ll, lr) {
        (None, None) => vec![],
        (None, Some(lr)) => next_indices(table_l, table_r, Some(lr), Some(r)),
        (Some(ll), None) => next_indices(table_l, table_r, Some(ll), Some(r)),
        (Some(ll), Some(lr)) => {
            if to_right(&table_l[l].prefix, &table_r[r].prefix) {
                let mut idxes = next_indices(table_l, table_r, Some(lr), Some(r));
                idxes.push(DifferenceIndex::OnlyL(ll));
                idxes
            } else {
                let mut idxes = next_indices(table_l, table_r, Some(ll), Some(r));
                idxes.insert(0, DifferenceIndex::OnlyL(lr));
                idxes
            }
        }
    }
}

fn next_indices_first_b<'a, P: Prefix, L, R>(
    table_l: &'a Table<P, L>,
    table_r: &'a Table<P, R>,
    l: usize,
    r: usize,
    rl: Option<usize>,
    rr: Option<usize>,
) -> Vec<DifferenceIndex> {
    match (rl, rr) {
        (None, None) => vec![DifferenceIndex::OnlyL(l)],
        (None, Some(rr)) => next_indices(table_l, table_r, Some(l), Some(rr)),
        (Some(rl), None) => next_indices(table_l, table_r, Some(l), Some(rl)),
        (Some(rl), Some(rr)) => {
            if to_right(&table_r[r].prefix, &table_l[l].prefix) {
                next_indices(table_l, table_r, Some(l), Some(rr))
            } else {
                next_indices(table_l, table_r, Some(l), Some(rl))
            }
        }
    }
}

fn extend_lpm<'a, P: Prefix, R>(
    table_r: &'a Table<P, R>,
    lpm_r: Option<(&'a P, &'a R)>,
    indices: impl IntoIterator<Item = DifferenceIndex> + 'static,
) -> impl Iterator<Item = (DifferenceIndex, Option<(&'a P, &'a R)>)> + 'a {
    let get_lpm_r = move |r: usize| table_r[r].prefix_value().or(lpm_r);
    indices.into_iter().map(move |x| match x {
        DifferenceIndex::Both(_, r) | DifferenceIndex::FirstR(_, r) => (x, get_lpm_r(r)),
        DifferenceIndex::FirstL(_, _) | DifferenceIndex::OnlyL(_) => (x, lpm_r),
    })
}
