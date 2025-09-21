use crate::to_right;

use super::*;

/// an iterator over the intersection of two [`crate::PrefixSet`]s in lexicographic order.
pub struct Intersection<'a, P, L, R> {
    pub(super) table_l: &'a Table<P, L>,
    pub(super) table_r: &'a Table<P, R>,
    pub(super) nodes: Vec<IntersectionIndex>,
}

/// an iterator over the intersection of two [`crate::PrefixSet`]s in lexicographic order, yielding
/// mutable references to all elements.
pub struct IntersectionMut<'a, P, L, R> {
    pub(super) table_l: &'a Table<P, L>,
    pub(super) table_r: &'a Table<P, R>,
    pub(super) nodes: Vec<IntersectionIndex>,
}

impl<'a, P, L, R> IntersectionMut<'a, P, L, R> {
    /// Safety:
    /// 1. Table_l must come from a `TrieViewMut` and satisfy the conditions in `TrieViewMut::new`.
    /// 2. Table_r must come from a `TrieViewMut` and satisfy the conditions in `TrieViewMut::new`.
    /// 3. Table_l and Table_r must be distinct. This is implicitly given by the two rules above.
    unsafe fn new(
        table_l: &'a Table<P, L>,
        table_r: &'a Table<P, R>,
        nodes: Vec<IntersectionIndex>,
    ) -> Self {
        Self {
            table_l,
            table_r,
            nodes,
        }
    }
}

pub(super) enum IntersectionIndex {
    Both(usize, usize),
    FirstA(usize, usize),
    FirstB(usize, usize),
}

impl<'a, P, L> TrieView<'a, P, L>
where
    P: Prefix,
{
    /// Iterate over the union of both Views. Each element will yield a reference to the prefix and
    /// the value stored in `self` and `other` (if the prefix is in both views).
    ///
    /// ```
    /// # use prefix_trie::*;
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
    ///     (net!("192.168.0.0/24"), "d"),
    ///     (net!("192.168.2.0/24"), "e"),
    /// ]);
    /// let sub_a = map_a.view_at(net!("192.168.0.0/22")).unwrap();
    /// let sub_b = map_b.view_at(net!("192.168.0.0/22")).unwrap();
    /// assert_eq!(
    ///     sub_a.intersection(sub_b).collect::<Vec<_>>(),
    ///     vec![
    ///         (&net!("192.168.0.0/22"), &2, &"b"),
    ///         (&net!("192.168.0.0/24"), &3, &"d"),
    ///     ]
    /// );
    /// # }
    /// ```
    pub fn intersection<R>(&self, other: impl AsView<'a, P, R>) -> Intersection<'a, P, L, R> {
        let other = other.view();
        Intersection {
            table_l: self.table,
            table_r: other.table,
            nodes: Vec::from_iter(next_indices(
                self.table,
                other.table,
                Some(self.loc.idx()),
                Some(other.loc.idx()),
            )),
        }
    }
}

impl<P, L> TrieViewMut<'_, P, L>
where
    P: Prefix,
{
    /// Iterate over the union of both Views. Each element will yield a reference to the prefix and
    /// mutable references to the values stored in `self` and `other` (if the prefix is in both
    /// views).
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # use prefix_trie::trieview::UnionItem;
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
    /// let mut map_b: PrefixMap<ipnet::Ipv4Net, usize> = PrefixMap::from_iter([
    ///     (net!("192.168.0.0/22"), 10),
    ///     (net!("192.168.0.0/23"), 20),
    ///     (net!("192.168.2.0/24"), 30),
    /// ]);
    ///
    /// // Modify the two maps by adding their values for elements of the same prefix.
    /// for (_, l, r) in map_a.view_mut().intersection_mut(&mut map_b) {
    ///     *l += *r;
    ///     *r = *l;
    /// }
    ///
    /// assert_eq!(
    ///     map_a.into_iter().collect::<Vec<_>>(),
    ///     vec![
    ///         (net!("192.168.0.0/20"), 1),
    ///         (net!("192.168.0.0/22"), 12),
    ///         (net!("192.168.0.0/24"), 3),
    ///         (net!("192.168.2.0/23"), 4),
    ///     ],
    /// );
    /// assert_eq!(
    ///     map_b.into_iter().collect::<Vec<_>>(),
    ///     vec![
    ///         (net!("192.168.0.0/22"), 12),
    ///         (net!("192.168.0.0/23"), 20),
    ///         (net!("192.168.2.0/24"), 30),
    ///     ],
    /// );
    /// # }
    /// ```
    pub fn intersection_mut<'b, R>(
        &'b mut self,
        other: impl AsViewMut<'b, P, R>,
    ) -> IntersectionMut<'b, P, L, R> {
        let other = other.view_mut();
        let nodes = Vec::from_iter(next_indices(
            self.table,
            other.table,
            Some(self.loc.idx()),
            Some(other.loc.idx()),
        ));
        // Safety: Both `self` and `other` are `TrieViewMut`s, and must adhere to the safety
        // constraints in `TrieViewMut::new`.
        unsafe { IntersectionMut::new(self.table, other.table, nodes) }
    }
}

impl<'a, P: Prefix, L, R> Iterator for Intersection<'a, P, L, R> {
    type Item = (&'a P, &'a L, &'a R);

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(cur) = self.nodes.pop() {
            match cur {
                IntersectionIndex::Both(l, r) => {
                    let node_l = &self.table_l[l];
                    let node_r = &self.table_r[r];
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
                    if let (Some(left), Some(right)) =
                        (node_l.value.as_ref(), node_r.value.as_ref())
                    {
                        return Some((&node_l.prefix, left, right));
                    }
                }
                IntersectionIndex::FirstA(l, r) => {
                    let node_l = &self.table_l[l];
                    self.nodes.extend(next_indices_first_a(
                        self.table_l,
                        self.table_r,
                        l,
                        node_l.left,
                        node_l.right,
                        r,
                    ));
                }
                IntersectionIndex::FirstB(l, r) => {
                    let node_r = &self.table_r[r];
                    self.nodes.extend(next_indices_first_b(
                        self.table_l,
                        self.table_r,
                        l,
                        r,
                        node_r.left,
                        node_r.right,
                    ));
                }
            }
        }
        None
    }
}

impl<'a, P: Prefix, L, R> Iterator for IntersectionMut<'a, P, L, R> {
    type Item = (&'a P, &'a mut L, &'a mut R);

    fn next(&mut self) -> Option<Self::Item> {
        // safety: map is a tree. Every node is visited exactly once during the iteration
        // (self.nodes is not public). Therefore, each in each iteration of this loop (also between
        // multiple calls to `next`), the index `cur` is different to any of the earlier
        // iterations. It is therefore safe to extend the lifetime of the elements to 'a (which is
        // the lifetime for which `self` has an exclusive reference over the map).
        while let Some(cur) = self.nodes.pop() {
            match cur {
                IntersectionIndex::Both(l, r) => {
                    let node_l = &self.table_l[l];
                    let node_r = &self.table_r[r];
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
                    let node_r = unsafe { self.table_r.get_mut(r) };
                    if let (Some(left), Some(right)) =
                        (node_l.value.as_mut(), node_r.value.as_mut())
                    {
                        return Some((&node_l.prefix, left, right));
                    }
                }
                IntersectionIndex::FirstA(l, r) => {
                    let node_l = &self.table_l[l];
                    self.nodes.extend(next_indices_first_a(
                        self.table_l,
                        self.table_r,
                        l,
                        node_l.left,
                        node_l.right,
                        r,
                    ));
                }
                IntersectionIndex::FirstB(l, r) => {
                    let node_r = &self.table_r[r];
                    self.nodes.extend(next_indices_first_b(
                        self.table_l,
                        self.table_r,
                        l,
                        r,
                        node_r.left,
                        node_r.right,
                    ));
                }
            }
        }
        None
    }
}

fn next_indices<'a, P: Prefix, L, R>(
    table_l: &'a Table<P, L>,
    table_r: &'a Table<P, R>,
    node_l: Option<usize>,
    node_r: Option<usize>,
) -> Option<IntersectionIndex> {
    match (node_l, node_r) {
        (None, Some(_)) => None,
        (Some(_), None) => None,
        (Some(a), Some(b)) => {
            let p_a = &table_l[a].prefix;
            let p_b = &table_r[b].prefix;
            if p_a.prefix_len() == p_b.prefix_len() {
                match p_a.mask().cmp(&p_b.mask()) {
                    std::cmp::Ordering::Equal => Some(IntersectionIndex::Both(a, b)),
                    _ => None,
                }
            } else if p_a.contains(p_b) {
                Some(IntersectionIndex::FirstA(a, b))
            } else if p_b.contains(p_a) {
                Some(IntersectionIndex::FirstB(a, b))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn next_indices_first_a<'a, P: Prefix, L, R>(
    table_l: &'a Table<P, L>,
    table_r: &'a Table<P, R>,
    l: usize,
    ll: Option<usize>,
    lr: Option<usize>,
    r: usize,
) -> Option<IntersectionIndex> {
    match (ll, lr) {
        (None, None) => None,
        (None, Some(lr)) => next_indices(table_l, table_r, Some(lr), Some(r)),
        (Some(ll), None) => next_indices(table_l, table_r, Some(ll), Some(r)),
        (Some(ll), Some(lr)) => {
            if to_right(&table_l[l].prefix, &table_r[r].prefix) {
                next_indices(table_l, table_r, Some(lr), Some(r))
            } else {
                next_indices(table_l, table_r, Some(ll), Some(r))
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
) -> Option<IntersectionIndex> {
    match (rl, rr) {
        (None, None) => None,
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
