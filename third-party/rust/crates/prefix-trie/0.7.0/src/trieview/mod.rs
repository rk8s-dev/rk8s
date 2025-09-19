//! A [`TrieView`] (or a [`TrieViewMut`]) is a pointer to a specific element in a PrefixTrie, representing the sub-tree
//! rooted at that node.
//!
//! This module allows you to perform Set operations (union, intersection, difference) on
//! [`PrefixMap`]s and [`PrefixSet`]s, optionally of only a trie-view.

use crate::{
    inner::{Direction, DirectionForInsert, Node, Table},
    map::{Iter, IterMut, Keys, Values, ValuesMut},
    to_right, Prefix, PrefixMap, PrefixSet,
};

/// A trait for creating a [`TrieView`] of `self`.
pub trait AsView<'a, P: Prefix, T>: Sized {
    /// Get a TrieView rooted at the origin (referencing the entire trie).
    fn view(self) -> TrieView<'a, P, T>;

    /// Get a TrieView rooted at the given `prefix`. If that `prefix` is not part of the trie, `None`
    /// is returned. Calling this function is identical to `self.view().find(prefix)`.
    fn view_at(self, prefix: P) -> Option<TrieView<'a, P, T>> {
        self.view().find(prefix)
    }
}

impl<'a, P: Prefix, T> AsView<'a, P, T> for TrieView<'a, P, T> {
    fn view(self) -> TrieView<'a, P, T> {
        self
    }
}

impl<'b: 'a, 'a, P: Prefix + Clone, T> AsView<'a, P, T> for &'a TrieViewMut<'b, P, T> {
    fn view(self) -> TrieView<'a, P, T> {
        TrieView {
            table: self.table,
            loc: self.loc.clone(),
        }
    }
}

impl<'a, P: Prefix, T> AsView<'a, P, T> for &'a PrefixMap<P, T> {
    fn view(self) -> TrieView<'a, P, T> {
        TrieView {
            table: &self.table,
            loc: ViewLoc::Node(0),
        }
    }
}

impl<'a, P: Prefix> AsView<'a, P, ()> for &'a PrefixSet<P> {
    fn view(self) -> TrieView<'a, P, ()> {
        TrieView {
            table: &self.0.table,
            loc: ViewLoc::Node(0),
        }
    }
}

/// A subtree of a prefix-trie rooted at a specific node.
///
/// The view can point to one of three possible things:
/// - A node in the tree that is actually present in the map,
/// - A branching node that does not exist in the map, but is needed for the tree structure (or that
///   was deleted using the function `remove_keep_tree`)
/// - A virtual node that does not exist as a node in the tree. This is only the case if you call
///   [`TrieView::find`] or [`AsView::view_at`] with a node that is not present in the tree, but
///   that contains elements present in the tree. Virtual nodes are treated as if they are actually
///   present in the tree as branching nodes.
pub struct TrieView<'a, P, T> {
    table: &'a Table<P, T>,
    loc: ViewLoc<P>,
}

#[derive(Clone, Copy)]
enum ViewLoc<P> {
    Node(usize),
    Virtual(P, usize),
}

impl<P> ViewLoc<P> {
    fn idx(&self) -> usize {
        match self {
            ViewLoc::Node(i) | ViewLoc::Virtual(_, i) => *i,
        }
    }
}

impl<P: Copy, T> Copy for TrieView<'_, P, T> {}

impl<P: Clone, T> Clone for TrieView<'_, P, T> {
    fn clone(&self) -> Self {
        Self {
            table: self.table,
            loc: self.loc.clone(),
        }
    }
}

impl<P: std::fmt::Debug, T: std::fmt::Debug> std::fmt::Debug for TrieView<'_, P, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("View").field(self.prefix()).finish()
    }
}

impl<'a, P, T> TrieView<'a, P, T>
where
    P: Prefix,
{
    /// Find `prefix`, returning a new view that points to the first node that is contained
    /// within that prefix (or `prefix` itself). Only the current view is searched. If `prefix`
    /// is not present in the current view referenced by `self` (including any sub-prefix of
    /// `prefix`), the function returns `None`.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// macro_rules! net { ($x:literal) => {$x.parse::<ipnet::Ipv4Net>().unwrap()}; }
    ///
    /// # #[cfg(feature = "ipnet")]
    /// # {
    /// let mut map: PrefixMap<ipnet::Ipv4Net, usize> = PrefixMap::from_iter([
    ///     (net!("192.168.0.0/20"), 1),
    ///     (net!("192.168.0.0/22"), 2),
    ///     (net!("192.168.0.0/24"), 3),
    ///     (net!("192.168.2.0/23"), 4),
    ///     (net!("192.168.4.0/22"), 5),
    /// ]);
    /// let sub = map.view();
    /// assert_eq!(
    ///     sub.find(net!("192.168.0.0/21")).unwrap().keys().collect::<Vec<_>>(),
    ///     vec![
    ///         &net!("192.168.0.0/22"),
    ///         &net!("192.168.0.0/24"),
    ///         &net!("192.168.2.0/23"),
    ///         &net!("192.168.4.0/22"),
    ///     ]
    /// );
    /// assert_eq!(
    ///     sub.find(net!("192.168.0.0/22")).unwrap().keys().collect::<Vec<_>>(),
    ///     vec![
    ///         &net!("192.168.0.0/22"),
    ///         &net!("192.168.0.0/24"),
    ///         &net!("192.168.2.0/23"),
    ///     ]
    /// );
    /// # }
    /// ```
    pub fn find(&self, prefix: P) -> Option<TrieView<'a, P, T>> {
        let mut idx = self.loc.idx();
        loop {
            match self.table.get_direction_for_insert(idx, &prefix) {
                DirectionForInsert::Enter { next, .. } => {
                    idx = next;
                }
                DirectionForInsert::Reached => {
                    return Some(Self {
                        table: self.table,
                        loc: ViewLoc::Node(idx),
                    })
                }
                DirectionForInsert::NewChild { right, .. } => {
                    // view at a virtual node between idx and the right child of idx.
                    return Some(Self {
                        table: self.table,
                        loc: ViewLoc::Virtual(prefix, self.table.get_child(idx, right).unwrap()),
                    });
                }
                DirectionForInsert::NewLeaf { .. } | DirectionForInsert::NewBranch { .. } => {
                    return None
                }
            }
        }
    }

    /// Find `prefix`, returning a new view that points to that node. Only the current view is
    /// searched. If this prefix is not present in the view pointed to by `self`, the function
    /// returns `None`.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// macro_rules! net { ($x:literal) => {$x.parse::<ipnet::Ipv4Net>().unwrap()}; }
    ///
    /// # #[cfg(feature = "ipnet")]
    /// # {
    /// let mut map: PrefixMap<ipnet::Ipv4Net, usize> = PrefixMap::from_iter([
    ///     (net!("192.168.0.0/20"), 1),
    ///     (net!("192.168.0.0/22"), 2),
    ///     (net!("192.168.0.0/24"), 3),
    ///     (net!("192.168.2.0/23"), 4),
    ///     (net!("192.168.4.0/22"), 5),
    /// ]);
    /// let sub = map.view();
    /// assert!(sub.find_exact(&net!("192.168.0.0/21")).is_none());
    /// assert_eq!(
    ///     sub.find_exact(&net!("192.168.0.0/22")).unwrap().keys().collect::<Vec<_>>(),
    ///     vec![
    ///         &net!("192.168.0.0/22"),
    ///         &net!("192.168.0.0/24"),
    ///         &net!("192.168.2.0/23"),
    ///     ]
    /// );
    /// # }
    /// ```
    pub fn find_exact(&self, prefix: &P) -> Option<TrieView<'a, P, T>> {
        let mut idx = self.loc.idx();
        loop {
            match self.table.get_direction(idx, prefix) {
                Direction::Reached => {
                    return self.table[idx].value.is_some().then_some(Self {
                        table: self.table,
                        loc: ViewLoc::Node(idx),
                    })
                }
                Direction::Enter { next, .. } => idx = next,
                Direction::Missing => return None,
            }
        }
    }

    /// Find the longest match of `prefix`, returning a new view that points to that node. Only
    /// the given view is searched. If the prefix is not present in the view pointed to by
    /// `self`, the function returns `None`.
    ///
    /// Only views to nodes that are present in the map are returned, not to branching nodes.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// macro_rules! net { ($x:literal) => {$x.parse::<ipnet::Ipv4Net>().unwrap()}; }
    ///
    /// # #[cfg(feature = "ipnet")]
    /// # {
    /// let mut map: PrefixMap<ipnet::Ipv4Net, usize> = PrefixMap::from_iter([
    ///     (net!("192.168.0.0/20"), 1),
    ///     (net!("192.168.0.0/22"), 2),
    ///     (net!("192.168.0.0/24"), 3),
    ///     (net!("192.168.2.0/23"), 4),
    ///     (net!("192.168.4.0/22"), 5),
    /// ]);
    /// let sub = map.view();
    /// assert_eq!(
    ///     sub.find_lpm(&net!("192.168.0.0/21")).unwrap().keys().collect::<Vec<_>>(),
    ///     vec![
    ///         &net!("192.168.0.0/20"),
    ///         &net!("192.168.0.0/22"),
    ///         &net!("192.168.0.0/24"),
    ///         &net!("192.168.2.0/23"),
    ///         &net!("192.168.4.0/22"),
    ///     ]
    /// );
    /// assert_eq!(
    ///     sub.find_lpm(&net!("192.168.0.0/22")).unwrap().keys().collect::<Vec<_>>(),
    ///     vec![
    ///         &net!("192.168.0.0/22"),
    ///         &net!("192.168.0.0/24"),
    ///         &net!("192.168.2.0/23"),
    ///     ]
    /// );
    /// # }
    /// ```
    pub fn find_lpm(&self, prefix: &P) -> Option<TrieView<'a, P, T>> {
        let mut idx = self.loc.idx();
        let mut best_match = None;
        loop {
            if self.table[idx].value.is_some() {
                best_match = Some(idx);
            }
            match self.table.get_direction(idx, prefix) {
                Direction::Enter { next, .. } => idx = next,
                _ => {
                    return best_match.map(|idx| Self {
                        table: self.table,
                        loc: ViewLoc::Node(idx),
                    })
                }
            }
        }
    }

    /// Get the left branch at the current view. The right branch contains all prefix that are
    /// contained within `self.prefix()`, and for which the next bit is set to 0.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// macro_rules! net { ($x:literal) => {$x.parse::<ipnet::Ipv4Net>().unwrap()}; }
    ///
    /// # #[cfg(feature = "ipnet")]
    /// # {
    /// let map: PrefixSet<ipnet::Ipv4Net> = PrefixSet::from_iter([
    ///     net!("1.0.0.0/8"),
    ///     net!("1.0.0.0/16"),
    ///     net!("1.0.128.0/17"),
    ///     net!("1.1.0.0/16"),
    /// ]);
    ///
    /// let view = map.view_at(net!("1.0.0.0/8")).unwrap();
    /// assert_eq!(view.prefix(), &net!("1.0.0.0/8"));
    ///
    /// let view = view.left().unwrap();
    /// assert_eq!(view.prefix(), &net!("1.0.0.0/15"));
    ///
    /// let view = view.left().unwrap();
    /// assert_eq!(view.prefix(), &net!("1.0.0.0/16"));
    ///
    /// assert!(view.left().is_none());
    /// # }
    /// ```
    pub fn left(&self) -> Option<Self> {
        match &self.loc {
            ViewLoc::Node(idx) => Some(Self {
                table: self.table,
                loc: ViewLoc::Node(self.table[*idx].left?),
            }),
            ViewLoc::Virtual(p, idx) => {
                // first, check if the node is on the left of the virtual one.
                if !to_right(p, &self.table[*idx].prefix) {
                    Some(Self {
                        table: self.table,
                        loc: ViewLoc::Node(*idx),
                    })
                } else {
                    None
                }
            }
        }
    }

    /// Get the right branch at the current view. The right branch contains all prefix that are
    /// contained within `self.prefix()`, and for which the next bit is set to 1.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// macro_rules! net { ($x:literal) => {$x.parse::<ipnet::Ipv4Net>().unwrap()}; }
    ///
    /// # #[cfg(feature = "ipnet")]
    /// # {
    /// let map: PrefixSet<ipnet::Ipv4Net> = PrefixSet::from_iter([
    ///     net!("1.0.0.0/8"),
    ///     net!("1.0.0.0/16"),
    ///     net!("1.1.0.0/16"),
    ///     net!("1.1.0.0/24"),
    /// ]);
    ///
    /// let view = map.view_at(net!("1.0.0.0/8")).unwrap();
    /// assert_eq!(view.prefix(), &net!("1.0.0.0/8"));
    ///
    /// assert!(view.right().is_none());
    /// let view = view.left().unwrap();
    /// assert_eq!(view.prefix(), &net!("1.0.0.0/15"));
    ///
    /// let view = view.right().unwrap();
    /// assert_eq!(view.prefix(), &net!("1.1.0.0/16"));
    ///
    /// assert!(view.right().is_none());
    /// # }
    /// ```
    pub fn right(&self) -> Option<Self> {
        match &self.loc {
            ViewLoc::Node(idx) => Some(Self {
                table: self.table,
                loc: ViewLoc::Node(self.table[*idx].right?),
            }),
            ViewLoc::Virtual(p, idx) => {
                // first, check if the node is on the right of the virtual one.
                if to_right(p, &self.table[*idx].prefix) {
                    Some(Self {
                        table: self.table,
                        loc: ViewLoc::Node(*idx),
                    })
                } else {
                    None
                }
            }
        }
    }
}

impl<'a, P, T> TrieView<'a, P, T> {
    /// Iterate over all elements in the given view (including the element itself), in
    /// lexicographic order.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// macro_rules! net { ($x:literal) => {$x.parse::<ipnet::Ipv4Net>().unwrap()}; }
    ///
    /// # #[cfg(feature = "ipnet")]
    /// # {
    /// let mut map: PrefixMap<ipnet::Ipv4Net, usize> = PrefixMap::from_iter([
    ///     (net!("192.168.0.0/20"), 1),
    ///     (net!("192.168.0.0/22"), 2),
    ///     (net!("192.168.0.0/24"), 3),
    ///     (net!("192.168.2.0/23"), 4),
    /// ]);
    /// let sub = map.view_at(net!("192.168.0.0/22")).unwrap();
    /// assert_eq!(
    ///     sub.iter().collect::<Vec<_>>(),
    ///     vec![
    ///         (&net!("192.168.0.0/22"), &2),
    ///         (&net!("192.168.0.0/24"), &3),
    ///         (&net!("192.168.2.0/23"), &4),
    ///     ]
    /// );
    /// # }
    /// ```
    pub fn iter(&self) -> Iter<'a, P, T> {
        Iter::new(self.table, vec![self.loc.idx()])
    }

    /// Iterate over all keys in the given view (including the element itself), in lexicographic
    /// order.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// macro_rules! net { ($x:literal) => {$x.parse::<ipnet::Ipv4Net>().unwrap()}; }
    ///
    /// # #[cfg(feature = "ipnet")]
    /// # {
    /// let mut map: PrefixMap<ipnet::Ipv4Net, usize> = PrefixMap::from_iter([
    ///     (net!("192.168.0.0/20"), 1),
    ///     (net!("192.168.0.0/22"), 2),
    ///     (net!("192.168.0.0/24"), 3),
    ///     (net!("192.168.2.0/23"), 4),
    /// ]);
    /// let sub = map.view_at(net!("192.168.0.0/22")).unwrap();
    /// assert_eq!(
    ///     sub.keys().collect::<Vec<_>>(),
    ///     vec![&net!("192.168.0.0/22"), &net!("192.168.0.0/24"), &net!("192.168.2.0/23")]
    /// );
    /// # }
    /// ```
    pub fn keys(&self) -> Keys<'a, P, T> {
        Keys { inner: self.iter() }
    }

    /// Iterate over all values in the given view (including the element itself), in lexicographic
    /// order.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// macro_rules! net { ($x:literal) => {$x.parse::<ipnet::Ipv4Net>().unwrap()}; }
    ///
    /// # #[cfg(feature = "ipnet")]
    /// # {
    /// let mut map: PrefixMap<ipnet::Ipv4Net, usize> = PrefixMap::from_iter([
    ///     (net!("192.168.0.0/20"), 1),
    ///     (net!("192.168.0.0/22"), 2),
    ///     (net!("192.168.0.0/24"), 3),
    ///     (net!("192.168.2.0/23"), 4),
    /// ]);
    /// let sub = map.view_at(net!("192.168.0.0/22")).unwrap();
    /// assert_eq!(sub.values().collect::<Vec<_>>(), vec![&2, &3, &4]);
    /// # }
    /// ```
    pub fn values(&self) -> Values<'a, P, T> {
        Values { inner: self.iter() }
    }

    /// Get a reference to the prefix that is currently pointed at. This prefix might not exist
    /// explicitly in the map/set, but may be used as a branching node (or when you call
    /// `remove_keep_tree`).
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// macro_rules! net { ($x:literal) => {$x.parse::<ipnet::Ipv4Net>().unwrap()}; }
    ///
    /// # #[cfg(feature = "ipnet")]
    /// # {
    /// let mut map: PrefixMap<ipnet::Ipv4Net, usize> = PrefixMap::from_iter([
    ///     (net!("192.168.0.0/20"), 1),
    ///     (net!("192.168.0.0/22"), 2),
    /// ]);
    ///
    /// assert_eq!(map.view_at(net!("192.168.0.0/20")).unwrap().prefix(), &net!("192.168.0.0/20"));
    /// assert_eq!(map.view_at(net!("192.168.0.0/21")).unwrap().prefix(), &net!("192.168.0.0/21"));
    /// assert_eq!(map.view_at(net!("192.168.0.0/22")).unwrap().prefix(), &net!("192.168.0.0/22"));
    /// # }
    /// ```
    pub fn prefix(&self) -> &P {
        match &self.loc {
            ViewLoc::Node(idx) => &self.table[*idx].prefix,
            ViewLoc::Virtual(p, _) => p,
        }
    }

    /// Get a reference to the value at the root of the current view. This function may return
    /// `None` if `self` is pointing at a branching node.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// macro_rules! net { ($x:literal) => {$x.parse::<ipnet::Ipv4Net>().unwrap()}; }
    ///
    /// # #[cfg(feature = "ipnet")]
    /// # {
    /// let mut map: PrefixMap<ipnet::Ipv4Net, usize> = PrefixMap::from_iter([
    ///     (net!("192.168.0.0/20"), 1),
    ///     (net!("192.168.0.0/22"), 2),
    /// ]);
    ///
    /// assert_eq!(map.view_at(net!("192.168.0.0/20")).unwrap().value(), Some(&1));
    /// assert_eq!(map.view_at(net!("192.168.0.0/21")).unwrap().value(), None);
    /// assert_eq!(map.view_at(net!("192.168.0.0/22")).unwrap().value(), Some(&2));
    /// # }
    /// ```
    pub fn value(&self) -> Option<&'a T> {
        match &self.loc {
            ViewLoc::Node(idx) => self.table[*idx].value.as_ref(),
            ViewLoc::Virtual(_, _) => None,
        }
    }

    /// Get a reference to both the prefix and the value. This function may return `None` if either
    /// `self` is pointing at a branching node.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// macro_rules! net { ($x:literal) => {$x.parse::<ipnet::Ipv4Net>().unwrap()}; }
    ///
    /// # #[cfg(feature = "ipnet")]
    /// # {
    /// let mut map: PrefixMap<ipnet::Ipv4Net, usize> = PrefixMap::from_iter([
    ///     (net!("192.168.0.0/20"), 1),
    ///     (net!("192.168.0.0/22"), 2),
    /// ]);
    ///
    /// assert_eq!(
    ///     map.view_at(net!("192.168.0.0/20")).unwrap().prefix_value(),
    ///     Some((&net!("192.168.0.0/20"), &1))
    /// );
    /// assert_eq!(map.view_at(net!("192.168.0.0/21")).unwrap().prefix_value(), None);
    /// # }
    /// ```
    pub fn prefix_value(&self) -> Option<(&'a P, &'a T)> {
        match &self.loc {
            ViewLoc::Node(idx) => self.table[*idx].prefix_value(),
            ViewLoc::Virtual(_, _) => None,
        }
    }
}

impl<'a, P, T> IntoIterator for TrieView<'a, P, T> {
    type Item = (&'a P, &'a T);
    type IntoIter = Iter<'a, P, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// A trait for creating a [`TrieViewMut`] of `self`.
pub trait AsViewMut<'a, P: Prefix, T>: Sized {
    /// Get a mutable view rooted at the origin (referencing the entire trie).
    fn view_mut(self) -> TrieViewMut<'a, P, T>;

    /// Get a mutable view rooted at the given `prefix`. If that `prefix` is not part of the trie, `None`
    /// is returned. Calling this function is identical to `self.view().find(prefix)`.
    fn view_mut_at(self, prefix: P) -> Option<TrieViewMut<'a, P, T>> {
        self.view_mut().find(prefix).ok()
    }
}

impl<'a, P: Prefix, T> AsViewMut<'a, P, T> for TrieViewMut<'a, P, T> {
    fn view_mut(self) -> TrieViewMut<'a, P, T> {
        self
    }
}

impl<'a, P: Prefix, T> AsViewMut<'a, P, T> for &'a mut PrefixMap<P, T> {
    fn view_mut(self) -> TrieViewMut<'a, P, T> {
        // Safety: We borrow the prefixmap mutably here. Thus, this is the only mutable reference,
        // and we can create such a view to the root (referencing the entire tree mutably).
        unsafe { TrieViewMut::new(&self.table, ViewLoc::Node(0)) }
    }
}

impl<'a, P: Prefix> AsViewMut<'a, P, ()> for &'a mut PrefixSet<P> {
    fn view_mut(self) -> TrieViewMut<'a, P, ()> {
        self.0.view_mut()
    }
}

/// A mutable view of a prefix-trie rooted at a specific node.
///
/// **Note**: You can get a `TrieView` from `TrieViewMut` by calling [`AsView::view`]. This will
/// create a view that has an immutable reference to the mutable view. This allows you to temprarily
/// borrow the subtrie immutably (to perform set operations). Once all references are dropped, you
/// can still use the mutable reference.
///
/// ```
/// # use prefix_trie::*;
/// # #[cfg(feature = "ipnet")]
/// # macro_rules! net { ($x:literal) => {$x.parse::<ipnet::Ipv4Net>().unwrap()}; }
/// # #[cfg(feature = "ipnet")]
/// # {
/// # let mut map: PrefixMap<ipnet::Ipv4Net, usize> = PrefixMap::from_iter([
/// #     (net!("192.168.0.0/20"), 1),
/// #     (net!("192.168.0.0/22"), 2),
/// #     (net!("192.168.0.0/24"), 3),
/// #     (net!("192.168.2.0/23"), 4),
/// # ]);
/// # let other: PrefixMap<ipnet::Ipv4Net, usize> = PrefixMap::from_iter([
/// #     (net!("192.168.0.0/22"), 10),
/// #     (net!("192.168.0.0/23"), 20),
/// #     (net!("192.168.2.0/24"), 30),
/// # ]);
/// let mut view: TrieViewMut<_, _> = // ...;
/// # map.view_mut();
/// // find the first element that is in the view and in another map.
/// if let Some((p, _, _)) = view.view().union(&other).find_map(|u| u.both()) {
///     // remove that element from the map.
///     let p = p.clone();
///     view.find(p).unwrap().remove();
/// }
/// # }
/// ```
///
/// The view can point to one of three possible things:
/// - A node in the tree that is actually present in the map,
/// - A branching node that does not exist in the map, but is needed for the tree structure (or that
///   was deleted using the function `remove_keep_tree`)
/// - A virtual node that does not exist as a node in the tree. This is only the case if you call
///   [`TrieViewMut::find`] or [`AsViewMut::view_mut_at`] with a node that is not present in the
///   tree, but that contains elements present in the tree. Virtual nodes are treated as if they are
///   actually present in the tree as branching.
pub struct TrieViewMut<'a, P, T> {
    table: &'a Table<P, T>,
    loc: ViewLoc<P>,
}

impl<'a, P, T> TrieViewMut<'a, P, T> {
    /// # Safety
    /// - First, ensure that `'a` is tied to a mutable reference `&'a Table<P, T>`.
    /// - Second, you must guarantee that, if multiple `TrieViewMut` exist, all of them point to
    ///   nodes that are located on separate sub-trees. You must guarantee that no `TrieViewMut` is
    ///   contained within another `TrieViewMut` or `TrieView`. Also, you must guarantee that no
    ///   `TrieView` is contained within a `TrieViewMut`.
    unsafe fn new(table: &'a Table<P, T>, loc: ViewLoc<P>) -> Self {
        Self { table, loc }
    }
}

impl<P: std::fmt::Debug, T: std::fmt::Debug> std::fmt::Debug for TrieViewMut<'_, P, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ViewMut").field(self.prefix()).finish()
    }
}

impl<P, T> TrieViewMut<'_, P, T>
where
    P: Prefix,
{
    /// Find `prefix`, returning a new view that points to the first node that is contained
    /// within that prefix (or `prefix` itself). Only the current view is searched. If `prefix`
    /// is not present in the current view referenced by `self` (including any sub-prefix of
    /// `prefix`), the function returns the previous view as `Err(self)`.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// macro_rules! net { ($x:literal) => {$x.parse::<ipnet::Ipv4Net>().unwrap()}; }
    ///
    /// # #[cfg(feature = "ipnet")]
    /// # {
    /// let mut map: PrefixMap<ipnet::Ipv4Net, usize> = PrefixMap::from_iter([
    ///     (net!("192.168.0.0/20"), 1),
    ///     (net!("192.168.0.0/22"), 2),
    ///     (net!("192.168.0.0/24"), 3),
    ///     (net!("192.168.2.0/23"), 4),
    ///     (net!("192.168.4.0/22"), 5),
    /// ]);
    /// map.view_mut().find(net!("192.168.0.0/21")).unwrap().values_mut().for_each(|x| *x += 10);
    /// assert_eq!(
    ///     map.into_iter().collect::<Vec<_>>(),
    ///     vec![
    ///         (net!("192.168.0.0/20"), 1),
    ///         (net!("192.168.0.0/22"), 12),
    ///         (net!("192.168.0.0/24"), 13),
    ///         (net!("192.168.2.0/23"), 14),
    ///         (net!("192.168.4.0/22"), 15),
    ///     ]
    /// );
    /// # }
    /// ```
    pub fn find(self, prefix: P) -> Result<Self, Self> {
        // Safety: We own the entire sub-tree, including `idx` (which was reached from
        // `self.idx`). Here, we return a new TrieViewMut pointing to that node (which
        // is still not covered by any other view), while dropping `self`.

        let mut idx = self.loc.idx();
        loop {
            match self.table.get_direction_for_insert(idx, &prefix) {
                DirectionForInsert::Enter { next, .. } => {
                    idx = next;
                }
                DirectionForInsert::Reached => {
                    let new_loc = ViewLoc::Node(idx);
                    return unsafe { Ok(Self::new(self.table, new_loc)) };
                }
                DirectionForInsert::NewChild { right, .. } => {
                    // view at a virtual node between idx and the right child of idx.
                    let new_loc =
                        ViewLoc::Virtual(prefix, self.table.get_child(idx, right).unwrap());
                    return unsafe { Ok(Self::new(self.table, new_loc)) };
                }
                DirectionForInsert::NewLeaf { .. } | DirectionForInsert::NewBranch { .. } => {
                    return Err(self)
                }
            }
        }
    }

    /// Find `prefix`, returning a new view that points to that node. Only the current view is
    /// searched. If this prefix is not present in the view pointed to by `self`, the function
    /// returns the previous view as `Err(self)`.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// macro_rules! net { ($x:literal) => {$x.parse::<ipnet::Ipv4Net>().unwrap()}; }
    ///
    /// # #[cfg(feature = "ipnet")]
    /// # {
    /// let mut map: PrefixMap<ipnet::Ipv4Net, usize> = PrefixMap::from_iter([
    ///     (net!("192.168.0.0/20"), 1),
    ///     (net!("192.168.0.0/22"), 2),
    ///     (net!("192.168.0.0/24"), 3),
    ///     (net!("192.168.2.0/23"), 4),
    ///     (net!("192.168.4.0/22"), 5),
    /// ]);
    /// assert!(map.view_mut().find_exact(&net!("192.168.0.0/21")).is_err());
    /// map.view_mut().find_exact(&net!("192.168.0.0/22")).unwrap().values_mut().for_each(|x| *x += 10);
    /// assert_eq!(
    ///     map.into_iter().collect::<Vec<_>>(),
    ///     vec![
    ///         (net!("192.168.0.0/20"), 1),
    ///         (net!("192.168.0.0/22"), 12),
    ///         (net!("192.168.0.0/24"), 13),
    ///         (net!("192.168.2.0/23"), 14),
    ///         (net!("192.168.4.0/22"), 5),
    ///     ]
    /// );
    /// # }
    /// ```
    pub fn find_exact(self, prefix: &P) -> Result<Self, Self> {
        let mut idx = self.loc.idx();
        loop {
            match self.table.get_direction(idx, prefix) {
                Direction::Reached => {
                    return if self.table[idx].value.is_some() {
                        // Safety: We own the entire sub-tree, including `idx` (which was reached
                        // from `self.idx`). Here, we return a new TrieViewMut pointing to that node
                        // (which is still not covered by any other view), while dropping `self`.
                        unsafe { Ok(Self::new(self.table, ViewLoc::Node(idx))) }
                    } else {
                        Err(self)
                    };
                }
                Direction::Enter { next, .. } => idx = next,
                Direction::Missing => return Err(self),
            }
        }
    }

    /// Find the longest match of `prefix`, returning a new view that points to that node. Only
    /// the given view is searched. If the prefix is not present in the view pointed to by
    /// `self`, the function returns the previous view as `Err(self)`.
    ///
    /// Only views to nodes that are present in the map are returned, not to branching nodes.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// macro_rules! net { ($x:literal) => {$x.parse::<ipnet::Ipv4Net>().unwrap()}; }
    ///
    /// # #[cfg(feature = "ipnet")]
    /// # {
    /// let mut map: PrefixMap<ipnet::Ipv4Net, usize> = PrefixMap::from_iter([
    ///     (net!("192.168.0.0/20"), 1),
    ///     (net!("192.168.0.0/22"), 2),
    ///     (net!("192.168.0.0/24"), 3),
    ///     (net!("192.168.2.0/23"), 4),
    ///     (net!("192.168.4.0/22"), 5),
    /// ]);
    /// map.view_mut().find_lpm(&net!("192.168.0.0/22")).unwrap().values_mut().for_each(|x| *x += 10);
    /// map.view_mut().find_lpm(&net!("192.168.0.0/23")).unwrap().values_mut().for_each(|x| *x += 100);
    /// assert_eq!(
    ///     map.into_iter().collect::<Vec<_>>(),
    ///     vec![
    ///         (net!("192.168.0.0/20"), 1),
    ///         (net!("192.168.0.0/22"), 112),
    ///         (net!("192.168.0.0/24"), 113),
    ///         (net!("192.168.2.0/23"), 114),
    ///         (net!("192.168.4.0/22"), 5),
    ///     ]
    /// );
    /// # }
    /// ```
    pub fn find_lpm(self, prefix: &P) -> Result<Self, Self> {
        let mut idx = self.loc.idx();
        let mut best_match = None;
        loop {
            if self.table[idx].value.is_some() {
                best_match = Some(idx);
            }
            match self.table.get_direction(idx, prefix) {
                Direction::Enter { next, .. } => idx = next,
                _ => {
                    return if let Some(idx) = best_match {
                        // Safety: We own the entire sub-tree, including `idx` (which was reached
                        // from `self.idx`). Here, we return a new TrieViewMut pointing to that node
                        // (which is still not covered by any other view), while dropping `self`.
                        unsafe { Ok(Self::new(self.table, ViewLoc::Node(idx))) }
                    } else {
                        Err(self)
                    };
                }
            }
        }
    }

    /// Get the left branch at the current view. The right branch contains all prefix that are
    /// contained within `self.prefix()`, and for which the next bit is set to 0. If the node has no
    /// children to the left, the function will return the previous view as `Err(self)`.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// macro_rules! net { ($x:literal) => {$x.parse::<ipnet::Ipv4Net>().unwrap()}; }
    ///
    /// # #[cfg(feature = "ipnet")]
    /// # {
    /// let mut map: PrefixSet<ipnet::Ipv4Net> = PrefixSet::from_iter([
    ///     net!("1.0.0.0/8"),
    ///     net!("1.0.0.0/16"),
    ///     net!("1.0.128.0/17"),
    ///     net!("1.1.0.0/16"),
    /// ]);
    ///
    /// let view = map.view_mut_at(net!("1.0.0.0/8")).unwrap();
    /// assert_eq!(view.prefix(), &net!("1.0.0.0/8"));
    ///
    /// let view = view.left().unwrap();
    /// assert_eq!(view.prefix(), &net!("1.0.0.0/15"));
    ///
    /// let view = view.left().unwrap();
    /// assert_eq!(view.prefix(), &net!("1.0.0.0/16"));
    ///
    /// assert!(view.left().is_err());
    /// # }
    /// ```
    pub fn left(self) -> Result<Self, Self> {
        // Safety: We assume `self` was created while satisfying the safety conditions from
        // `TrieViewMut::new`. Thus, `self` is the only TrieView referencing that root. Here, we
        // construct a new `TrieViewMut` of the left child while destroying `self`, and thus,
        // the safety conditions remain satisfied.

        let left_idx = match &self.loc {
            ViewLoc::Node(idx) => self.table[*idx].left,
            ViewLoc::Virtual(p, idx) => {
                // first, check if the node is on the left of the virtual one.
                if !to_right(p, &self.table[*idx].prefix) {
                    Some(*idx)
                } else {
                    None
                }
            }
        };

        if let Some(idx) = left_idx {
            unsafe { Ok(Self::new(self.table, ViewLoc::Node(idx))) }
        } else {
            Err(self)
        }
    }

    /// Get the right branch at the current view. The right branch contains all prefix that are
    /// contained within `self.prefix()`, and for which the next bit is set to 1. If the node has no
    /// children to the right, the function will return the previous view as `Err(self)`.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// macro_rules! net { ($x:literal) => {$x.parse::<ipnet::Ipv4Net>().unwrap()}; }
    ///
    /// # #[cfg(feature = "ipnet")]
    /// # {
    /// let mut map: PrefixSet<ipnet::Ipv4Net> = PrefixSet::from_iter([
    ///     net!("1.0.0.0/8"),
    ///     net!("1.0.0.0/16"),
    ///     net!("1.1.0.0/16"),
    ///     net!("1.1.0.0/24"),
    /// ]);
    ///
    /// let view = map.view_mut_at(net!("1.0.0.0/8")).unwrap();
    /// assert_eq!(view.prefix(), &net!("1.0.0.0/8"));
    ///
    /// let view = view.right().unwrap_err(); // there is no view on the right.
    /// assert_eq!(view.prefix(), &net!("1.0.0.0/8"));
    ///
    /// let view = view.left().unwrap();
    /// assert_eq!(view.prefix(), &net!("1.0.0.0/15"));
    ///
    /// let view = view.right().unwrap();
    /// assert_eq!(view.prefix(), &net!("1.1.0.0/16"));
    ///
    /// assert!(view.right().is_err());
    /// # }
    /// ```
    pub fn right(self) -> Result<Self, Self> {
        // Safety: We assume `self` was created while satisfying the safety conditions from
        // `TrieViewMut::new`. Thus, `self` is the only TrieView referencing that root. Here, we
        // construct a new `TrieViewMut` of the right child while destroying `self`, and thus,
        // the safety conditions remain satisfied.

        let right_idx = match &self.loc {
            ViewLoc::Node(idx) => self.table[*idx].right,
            ViewLoc::Virtual(p, idx) => {
                // first, check if the node is on the right of the virtual one.
                if to_right(p, &self.table[*idx].prefix) {
                    Some(*idx)
                } else {
                    None
                }
            }
        };

        if let Some(idx) = right_idx {
            unsafe { Ok(Self::new(self.table, ViewLoc::Node(idx))) }
        } else {
            Err(self)
        }
    }

    /// Returns `True` whether `self` has children to the left.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// macro_rules! net { ($x:literal) => {$x.parse::<ipnet::Ipv4Net>().unwrap()}; }
    ///
    /// # #[cfg(feature = "ipnet")]
    /// # {
    /// let mut map: PrefixSet<ipnet::Ipv4Net> = PrefixSet::from_iter([
    ///     net!("1.0.0.0/8"),
    ///     net!("1.0.0.0/9"),
    /// ]);
    ///
    /// assert!(map.view_mut_at(net!("1.0.0.0/8")).unwrap().has_left());
    /// assert!(!map.view_mut_at(net!("1.0.0.0/9")).unwrap().has_left());
    /// # }
    /// ```
    pub fn has_left(&self) -> bool {
        match &self.loc {
            ViewLoc::Node(idx) => self.table[*idx].left.is_some(),
            ViewLoc::Virtual(p, idx) => {
                // first, check if the node is on the right of the virtual one.
                !to_right(p, &self.table[*idx].prefix)
            }
        }
    }

    /// Returns `True` whether `self` has children to the right.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// macro_rules! net { ($x:literal) => {$x.parse::<ipnet::Ipv4Net>().unwrap()}; }
    ///
    /// # #[cfg(feature = "ipnet")]
    /// # {
    /// let mut map: PrefixSet<ipnet::Ipv4Net> = PrefixSet::from_iter([
    ///     net!("1.0.0.0/8"),
    ///     net!("1.128.0.0/9"),
    /// ]);
    ///
    /// assert!(map.view_mut_at(net!("1.0.0.0/8")).unwrap().has_right());
    /// assert!(!map.view_mut_at(net!("1.128.0.0/9")).unwrap().has_right());
    /// # }
    /// ```
    pub fn has_right(&self) -> bool {
        match &self.loc {
            ViewLoc::Node(idx) => self.table[*idx].right.is_some(),
            ViewLoc::Virtual(p, idx) => {
                // first, check if the node is on the right of the virtual one.
                to_right(p, &self.table[*idx].prefix)
            }
        }
    }

    /// Split `self` into two views, one pointing to the left and one pointing to the right child.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// macro_rules! net { ($x:literal) => {$x.parse::<ipnet::Ipv4Net>().unwrap()}; }
    ///
    /// # #[cfg(feature = "ipnet")]
    /// # {
    /// let mut map: PrefixMap<ipnet::Ipv4Net, &'static str> = PrefixMap::from_iter([
    ///     (net!("1.0.0.0/8"), "a"),
    ///     (net!("1.0.0.0/9"), "b"),
    ///     (net!("1.128.0.0/9"), "c"),
    ///     (net!("1.128.0.0/10"), "d"),
    /// ]);
    ///
    /// let view_at_a = map.view_mut_at(net!("1.0.0.0/8")).unwrap();
    /// assert_eq!(view_at_a.value(), Some(&"a"));
    ///
    /// let (Some(view_at_b), Some(view_at_c)) = view_at_a.split() else { unreachable!() };
    /// assert_eq!(view_at_b.value(), Some(&"b"));
    /// assert_eq!(view_at_c.value(), Some(&"c"));
    ///
    /// let (Some(view_at_d), None) = view_at_c.split() else { unreachable!() };
    /// assert_eq!(view_at_d.value(), Some(&"d"));
    /// # }
    /// ```
    pub fn split(self) -> (Option<Self>, Option<Self>) {
        let (left, right) = match &self.loc {
            ViewLoc::Node(idx) => (self.table[*idx].left, self.table[*idx].right),
            ViewLoc::Virtual(p, idx) => {
                // check if the node is on the right or the left of the virtual one.
                if to_right(p, &self.table[*idx].prefix) {
                    (None, Some(*idx))
                } else {
                    (Some(*idx), None)
                }
            }
        };

        // Safety: We assume `self` was created while satisfying the safety conditions from
        // `TrieViewMut::new`. Thus, `self` is the only TrieView referencing that root. Here, we
        // construct two new `TrieViewMut`s, one on the left and one on the right. Thus, they are
        // siblings and don't overlap. Further, we destroy `self`, ensuring that the safety
        // guarantees remain satisfied.
        unsafe {
            (
                left.map(|idx| Self::new(self.table, ViewLoc::Node(idx))),
                right.map(|idx| Self::new(self.table, ViewLoc::Node(idx))),
            )
        }
    }
}

impl<P, T> TrieViewMut<'_, P, T> {
    /// Iterate over all elements in the given view (including the element itself), in
    /// lexicographic order, with a mutable reference to the value.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// macro_rules! net { ($x:literal) => {$x.parse::<ipnet::Ipv4Net>().unwrap()}; }
    ///
    /// # #[cfg(feature = "ipnet")]
    /// # {
    /// let mut map: PrefixMap<ipnet::Ipv4Net, usize> = PrefixMap::from_iter([
    ///     (net!("1.0.0.0/8"), 1),
    ///     (net!("1.0.0.0/16"), 2),
    ///     (net!("1.0.1.0/24"), 3),
    ///     (net!("1.1.0.0/16"), 4),
    /// ]);
    ///
    /// map.view_mut_at(net!("1.0.0.0/16")).unwrap().iter_mut().for_each(|(_, x)| *x *= 10);
    /// assert_eq!(
    ///     map.into_iter().collect::<Vec<_>>(),
    ///     vec![
    ///         (net!("1.0.0.0/8"), 1),
    ///         (net!("1.0.0.0/16"), 20),
    ///         (net!("1.0.1.0/24"), 30),
    ///         (net!("1.1.0.0/16"), 4),
    ///     ]
    /// );
    /// # }
    /// ```
    pub fn iter_mut(&mut self) -> IterMut<'_, P, T> {
        // Safety: Here, we assume the TrieView was created using the `TrieViewMut::new` function,
        // and that the safety conditions from that function were satisfied. These safety conditions
        // comply with the safety conditions from `IterMut::new()`. Further, `self` is borrowed
        // mutably for the lifetime of the mutable iterator.
        unsafe { IterMut::new(self.table, vec![self.loc.idx()]) }
    }

    /// Iterate over mutable references to all values in the given view (including the element
    /// itself), in lexicographic order.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// macro_rules! net { ($x:literal) => {$x.parse::<ipnet::Ipv4Net>().unwrap()}; }
    ///
    /// # #[cfg(feature = "ipnet")]
    /// # {
    /// let mut map: PrefixMap<ipnet::Ipv4Net, usize> = PrefixMap::from_iter([
    ///     (net!("192.168.0.0/20"), 1),
    ///     (net!("192.168.0.0/22"), 2),
    ///     (net!("192.168.0.0/24"), 3),
    ///     (net!("192.168.2.0/23"), 4),
    /// ]);
    ///
    /// map.view_mut_at(net!("192.168.0.0/22")).unwrap().values_mut().for_each(|x| *x *= 10);
    /// assert_eq!(
    ///     map.into_iter().collect::<Vec<_>>(),
    ///     vec![
    ///         (net!("192.168.0.0/20"), 1),
    ///         (net!("192.168.0.0/22"), 20),
    ///         (net!("192.168.0.0/24"), 30),
    ///         (net!("192.168.2.0/23"), 40),
    ///     ]
    /// );
    /// # }
    /// ```
    pub fn values_mut(&mut self) -> ValuesMut<'_, P, T> {
        ValuesMut {
            inner: self.iter_mut(),
        }
    }

    /// Get a reference to the prefix that is currently pointed at. This prefix might not exist
    /// explicitly in the map/set. Instead, it might be a branching or a virtual node. In both
    /// cases, this function returns the prefix of that node.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// macro_rules! net { ($x:literal) => {$x.parse::<ipnet::Ipv4Net>().unwrap()}; }
    ///
    /// # #[cfg(feature = "ipnet")]
    /// # {
    /// let mut map: PrefixMap<ipnet::Ipv4Net, usize> = PrefixMap::from_iter([
    ///     (net!("1.0.0.0/20"), 1),
    ///     (net!("1.0.0.0/22"), 2),
    /// ]);
    ///
    /// assert_eq!(map.view_mut_at(net!("1.0.0.0/20")).unwrap().prefix(), &net!("1.0.0.0/20"));
    /// assert_eq!(map.view_mut_at(net!("1.0.0.0/21")).unwrap().prefix(), &net!("1.0.0.0/21"));
    /// assert_eq!(map.view_mut_at(net!("1.0.0.0/22")).unwrap().prefix(), &net!("1.0.0.0/22"));
    /// # }
    /// ```
    pub fn prefix(&self) -> &P {
        match &self.loc {
            ViewLoc::Node(idx) => &self.table[*idx].prefix,
            ViewLoc::Virtual(p, _) => p,
        }
    }

    /// Get a reference to the value at the root of the current view. This function may return
    /// `None` if `self` is pointing at a branching or a virtual node.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// macro_rules! net { ($x:literal) => {$x.parse::<ipnet::Ipv4Net>().unwrap()}; }
    ///
    /// # #[cfg(feature = "ipnet")]
    /// # {
    /// let mut map: PrefixMap<ipnet::Ipv4Net, usize> = PrefixMap::from_iter([
    ///     (net!("1.0.0.0/20"), 1),
    ///     (net!("1.0.0.0/22"), 2),
    /// ]);
    ///
    /// assert_eq!(map.view_mut_at(net!("1.0.0.0/20")).unwrap().value(), Some(&1));
    /// assert_eq!(map.view_mut_at(net!("1.0.0.0/21")).unwrap().value(), None);
    /// assert_eq!(map.view_mut_at(net!("1.0.0.0/22")).unwrap().value(), Some(&2));
    /// # }
    /// ```
    pub fn value(&self) -> Option<&T> {
        match &self.loc {
            ViewLoc::Node(idx) => self.table[*idx].value.as_ref(),
            ViewLoc::Virtual(_, _) => None,
        }
    }

    fn node_mut(&mut self) -> Option<&mut Node<P, T>> {
        // Safety: In the following, we assume that the safety conditions of `TrieViewMut::new` were
        // satisfied. In that case, we know that we are the only ones owning a mutable reference to
        // a tree that contains that root node. Therefore, it is safe to take a mutable reference of
        // that value.
        match &self.loc {
            ViewLoc::Node(idx) => unsafe { Some(self.table.get_mut(*idx)) },
            ViewLoc::Virtual(_, _) => None,
        }
    }

    /// Get a mutable reference to the value at the root of the current view. This function may
    /// return `None` if `self` is pointing at a branching node.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// macro_rules! net { ($x:literal) => {$x.parse::<ipnet::Ipv4Net>().unwrap()}; }
    ///
    /// # #[cfg(feature = "ipnet")]
    /// # {
    /// let mut map: PrefixMap<ipnet::Ipv4Net, usize> = PrefixMap::from_iter([
    ///     (net!("1.0.0.0/20"), 1),
    ///     (net!("1.0.0.0/22"), 2),
    /// ]);
    /// *map.view_mut_at(net!("1.0.0.0/22")).unwrap().value_mut().unwrap() *= 10;
    /// assert_eq!(Vec::from_iter(map), vec![(net!("1.0.0.0/20"), 1), (net!("1.0.0.0/22"), 20)]);
    /// # }
    /// ```
    pub fn value_mut(&mut self) -> Option<&mut T> {
        self.node_mut()?.value.as_mut()
    }

    /// Get a reference to both the prefix and the value. This function may return `None` if either
    /// `self` is pointing at a branching node.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// macro_rules! net { ($x:literal) => {$x.parse::<ipnet::Ipv4Net>().unwrap()}; }
    ///
    /// # #[cfg(feature = "ipnet")]
    /// # {
    /// let mut map: PrefixMap<ipnet::Ipv4Net, usize> = PrefixMap::from_iter([
    ///     (net!("192.168.0.0/20"), 1),
    ///     (net!("192.168.0.0/22"), 2),
    /// ]);
    ///
    /// assert_eq!(
    ///     map.view_mut_at(net!("192.168.0.0/20")).unwrap().prefix_value(),
    ///     Some((&net!("192.168.0.0/20"), &1))
    /// );
    /// assert_eq!(map.view_mut_at(net!("192.168.0.0/21")).unwrap().prefix_value(), None);
    /// # }
    /// ```
    pub fn prefix_value(&self) -> Option<(&P, &T)> {
        match &self.loc {
            ViewLoc::Node(idx) => self.table[*idx].prefix_value(),
            ViewLoc::Virtual(_, _) => None,
        }
    }

    /// Get a reference to both the prefix and the value (the latter is mutable). This function may
    /// return `None` if either `self` is pointing at a branching node.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// macro_rules! net { ($x:literal) => {$x.parse::<ipnet::Ipv4Net>().unwrap()}; }
    ///
    /// # #[cfg(feature = "ipnet")]
    /// # {
    /// let mut map: PrefixMap<ipnet::Ipv4Net, usize> = PrefixMap::from_iter([
    ///     (net!("1.0.0.0/20"), 1),
    ///     (net!("1.0.0.0/22"), 2),
    /// ]);
    /// *map.view_mut_at(net!("1.0.0.0/22")).unwrap().prefix_value_mut().unwrap().1 *= 10;
    /// assert_eq!(Vec::from_iter(map), vec![(net!("1.0.0.0/20"), 1), (net!("1.0.0.0/22"), 20)]);
    /// # }
    /// ```
    pub fn prefix_value_mut(&mut self) -> Option<(&P, &mut T)> {
        self.node_mut()?.prefix_value_mut()
    }

    /// Remove the element at the current position of the view. The tree structure is not modified
    /// (similar to calling [`PrefixMap::remove_keep_tree`].)
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// macro_rules! net { ($x:literal) => {$x.parse::<ipnet::Ipv4Net>().unwrap()}; }
    ///
    /// # #[cfg(feature = "ipnet")]
    /// # {
    /// let mut map: PrefixMap<ipnet::Ipv4Net, usize> = PrefixMap::from_iter([
    ///     (net!("192.168.0.0/20"), 1),
    ///     (net!("192.168.0.0/22"), 2),
    ///     (net!("192.168.0.0/24"), 3),
    ///     (net!("192.168.2.0/23"), 4),
    ///     (net!("192.168.4.0/22"), 5),
    /// ]);
    /// let mut view = map.view_mut_at(net!("192.168.0.0/22")).unwrap();
    /// assert_eq!(view.remove(), Some(2));
    /// assert_eq!(
    ///     view.into_iter().collect::<Vec<_>>(),
    ///     vec![
    ///         (&net!("192.168.0.0/24"), &mut 3),
    ///         (&net!("192.168.2.0/23"), &mut 4),
    ///     ]
    /// );
    /// assert_eq!(
    ///     map.into_iter().collect::<Vec<_>>(),
    ///     vec![
    ///         (net!("192.168.0.0/20"), 1),
    ///         (net!("192.168.0.0/24"), 3),
    ///         (net!("192.168.2.0/23"), 4),
    ///         (net!("192.168.4.0/22"), 5),
    ///     ]
    /// );
    /// # }
    /// ```
    pub fn remove(&mut self) -> Option<T> {
        self.node_mut()?.value.take()
    }

    /// Set the value of the node currently pointed at. This operation fails if the current view
    /// points at a virtual node, returning `Err(value)`. In such a case, you may want to go to the
    /// next node (e.g., using [`TrieViewMut::split`]).
    ///
    /// This operation will only modify the value, and keep the prefix unchanged (in contrast to
    /// `PrefixMap::insert`).
    ///
    /// This is an implementation detail of mutable views. Since you can have multiple different
    /// mutable views pointing to different parts in the tree, it is not safe to modify the tree
    /// structure itself.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// macro_rules! net { ($x:literal) => {$x.parse::<ipnet::Ipv4Net>().unwrap()}; }
    ///
    /// # #[cfg(feature = "ipnet")]
    /// # {
    /// let mut map: PrefixMap<ipnet::Ipv4Net, usize> = PrefixMap::from_iter([
    ///     (net!("192.168.0.0/20"), 1),
    ///     (net!("192.168.0.0/22"), 2),
    ///     (net!("192.168.0.0/24"), 3),
    ///     (net!("192.168.2.0/23"), 4),
    ///     (net!("192.168.4.0/22"), 5),
    /// ]);
    /// let mut view = map.view_mut_at(net!("192.168.0.0/22")).unwrap();
    /// assert_eq!(view.set(20), Ok(Some(2)));
    /// assert_eq!(
    ///     view.into_iter().collect::<Vec<_>>(),
    ///     vec![
    ///         (&net!("192.168.0.0/22"), &mut 20),
    ///         (&net!("192.168.0.0/24"), &mut 3),
    ///         (&net!("192.168.2.0/23"), &mut 4),
    ///     ]
    /// );
    /// assert_eq!(
    ///     map.into_iter().collect::<Vec<_>>(),
    ///     vec![
    ///         (net!("192.168.0.0/20"), 1),
    ///         (net!("192.168.0.0/22"), 20),
    ///         (net!("192.168.0.0/24"), 3),
    ///         (net!("192.168.2.0/23"), 4),
    ///         (net!("192.168.4.0/22"), 5),
    ///     ]
    /// );
    /// # }
    /// ```
    pub fn set(&mut self, value: T) -> Result<Option<T>, T> {
        match self.node_mut() {
            Some(n) => Ok(n.value.replace(value)),
            None => Err(value),
        }
    }
}

impl<'a, P, T> IntoIterator for TrieViewMut<'a, P, T> {
    type Item = (&'a P, &'a mut T);
    type IntoIter = IterMut<'a, P, T>;

    fn into_iter(self) -> Self::IntoIter {
        // Safety: Here, we assume the TrieView was created using the `TrieViewMut::new` function,
        // and that the safety conditions from that function were satisfied. These safety conditions
        // comply with the safety conditions from `IterMut::new()`.
        unsafe { IterMut::new(self.table, vec![self.loc.idx()]) }
    }
}

mod difference;
mod intersection;
mod union;
pub use difference::{
    CoveringDifference, CoveringDifferenceMut, Difference, DifferenceItem, DifferenceMut,
    DifferenceMutItem,
};
pub use intersection::{Intersection, IntersectionMut};
pub use union::{Union, UnionItem, UnionMut};
