//! Code for inserting elements and the entry pattern.

use super::*;

/// A mutable view into a single entry in a map, which may either be vacant or occupied.
pub enum Entry<'a, P, T> {
    /// The entry is not present in the tree.
    Vacant(VacantEntry<'a, P, T>),
    /// The entry is already present in the tree.
    Occupied(OccupiedEntry<'a, P, T>),
}

/// A mutable view into a missing entry. The information within this structure describes a path
/// towards that missing node, and how to insert it.
pub struct VacantEntry<'a, P, T> {
    pub(super) map: &'a mut PrefixMap<P, T>,
    pub(super) prefix: P,
    pub(super) idx: usize,
    pub(super) direction: DirectionForInsert<P>,
}

/// A mutable view into an occupied entry. An occupied entry represents a node that is already
/// present on the tree.
pub struct OccupiedEntry<'a, P, T> {
    pub(super) node: &'a mut Node<P, T>,
    pub(super) prefix: P, // needed to replace the prefix on the thing if we perform insert.
}

impl<P, T> Entry<'_, P, T> {
    /// Get the value if it exists
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut pm: PrefixMap<ipnet::Ipv4Net, _> = PrefixMap::new();
    /// pm.insert("192.168.1.0/24".parse()?, 1);
    /// assert_eq!(pm.entry("192.168.1.0/24".parse()?).get(), Some(&1));
    /// assert_eq!(pm.entry("192.168.2.0/24".parse()?).get(), None);
    /// # Ok(())
    /// # }
    /// # #[cfg(not(feature = "ipnet"))]
    /// # fn main() {}
    /// ```
    pub fn get(&self) -> Option<&T> {
        match self {
            Entry::Vacant(_) => None,
            Entry::Occupied(e) => e.node.value.as_ref(),
        }
    }

    /// Get the value if it exists
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut pm: PrefixMap<ipnet::Ipv4Net, _> = PrefixMap::new();
    /// pm.insert("192.168.1.0/24".parse()?, 1);
    /// pm.entry("192.168.1.0/24".parse()?).get_mut().map(|x| *x += 1);
    /// pm.entry("192.168.2.0/24".parse()?).get_mut().map(|x| *x += 1);
    /// assert_eq!(pm.get(&"192.168.1.0/24".parse()?), Some(&2));
    /// assert_eq!(pm.get(&"192.168.2.0/24".parse()?), None);
    /// # Ok(())
    /// # }
    /// # #[cfg(not(feature = "ipnet"))]
    /// # fn main() {}
    /// ```
    pub fn get_mut(&mut self) -> Option<&mut T> {
        match self {
            Entry::Vacant(_) => None,
            Entry::Occupied(e) => e.node.value.as_mut(),
        }
    }

    /// get the key of the current entry
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut pm: PrefixMap<ipnet::Ipv4Net, _> = PrefixMap::new();
    /// pm.insert("192.168.1.0/24".parse()?, 1);
    /// assert_eq!(pm.entry("192.168.1.0/24".parse()?).key(), &"192.168.1.0/24".parse()?);
    /// assert_eq!(pm.entry("192.168.2.0/24".parse()?).key(), &"192.168.2.0/24".parse()?);
    /// # Ok(())
    /// # }
    /// # #[cfg(not(feature = "ipnet"))]
    /// # fn main() {}
    /// ```
    pub fn key(&self) -> &P {
        match self {
            Entry::Vacant(e) => &e.prefix,
            Entry::Occupied(e) => &e.node.prefix,
        }
    }
}

impl<'a, P, T> Entry<'a, P, T>
where
    P: Prefix,
{
    /// Replace the current entry, and return the entry that was stored before. This will also
    /// replace the key with the one provided to the `entry` function.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut pm: PrefixMap<ipnet::Ipv4Net, _> = PrefixMap::new();
    /// pm.insert("192.168.1.0/24".parse()?, 1);
    ///
    /// assert_eq!(pm.entry("192.168.1.0/24".parse()?).insert(10), Some(1));
    /// assert_eq!(pm.entry("192.168.2.0/24".parse()?).insert(20), None);
    ///
    /// assert_eq!(pm.get(&"192.168.1.0/24".parse()?), Some(&10));
    /// assert_eq!(pm.get(&"192.168.2.0/24".parse()?), Some(&20));
    /// # Ok(())
    /// # }
    /// # #[cfg(not(feature = "ipnet"))]
    /// # fn main() {}
    /// ```
    #[inline(always)]
    pub fn insert(self, v: T) -> Option<T> {
        match self {
            Entry::Vacant(e) => {
                e._insert(v);
                None
            }
            Entry::Occupied(e) => Some(e.insert(v)),
        }
    }

    /// Ensures a value is in the entry by inserting the default if empty, and returns a mutable
    /// reference to the value in the entry.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut pm: PrefixMap<ipnet::Ipv4Net, _> = PrefixMap::new();
    /// pm.insert("192.168.1.0/24".parse()?, 1);
    ///
    /// assert_eq!(pm.entry("192.168.1.0/24".parse()?).or_insert(10), &1);
    /// assert_eq!(pm.entry("192.168.2.0/24".parse()?).or_insert(20), &20);
    ///
    /// assert_eq!(pm.get(&"192.168.1.0/24".parse()?), Some(&1));
    /// assert_eq!(pm.get(&"192.168.2.0/24".parse()?), Some(&20));
    /// # Ok(())
    /// # }
    /// # #[cfg(not(feature = "ipnet"))]
    /// # fn main() {}
    /// ```
    #[inline(always)]
    pub fn or_insert(self, default: T) -> &'a mut T {
        match self {
            Entry::Vacant(e) => e._insert(default).value.as_mut().unwrap(),
            Entry::Occupied(e) => e.node.value.get_or_insert(default),
        }
    }

    /// Ensures a value is in the entry by inserting the result of the default function if empty,
    /// and returns a mutable reference to the value in the entry.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut pm: PrefixMap<ipnet::Ipv4Net, _> = PrefixMap::new();
    /// pm.insert("192.168.1.0/24".parse()?, 1);
    ///
    /// assert_eq!(pm.entry("192.168.1.0/24".parse()?).or_insert_with(|| 10), &1);
    /// assert_eq!(pm.entry("192.168.2.0/24".parse()?).or_insert_with(|| 20), &20);
    ///
    /// assert_eq!(pm.get(&"192.168.1.0/24".parse()?), Some(&1));
    /// assert_eq!(pm.get(&"192.168.2.0/24".parse()?), Some(&20));
    /// # Ok(())
    /// # }
    /// # #[cfg(not(feature = "ipnet"))]
    /// # fn main() {}
    /// ```
    #[inline(always)]
    pub fn or_insert_with<F: FnOnce() -> T>(self, default: F) -> &'a mut T {
        match self {
            Entry::Vacant(e) => e._insert(default()).value.as_mut().unwrap(),
            Entry::Occupied(e) => e.node.value.get_or_insert_with(default),
        }
    }

    /// Provides in-place mutable access to an occupied entry before any potential inserts into the
    /// map.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut pm: PrefixMap<ipnet::Ipv4Net, _> = PrefixMap::new();
    /// pm.insert("192.168.1.0/24".parse()?, 1);
    /// assert_eq!(pm.entry("192.168.1.0/24".parse()?).and_modify(|x| *x += 1).get(), Some(&2));
    /// assert_eq!(pm.entry("192.168.2.0/24".parse()?).and_modify(|x| *x += 1).get(), None);
    /// # Ok(())
    /// # }
    /// # #[cfg(not(feature = "ipnet"))]
    /// # fn main() {}
    /// ```
    #[inline(always)]
    pub fn and_modify<F: FnOnce(&mut T)>(self, f: F) -> Self {
        match self {
            Entry::Vacant(e) => Entry::Vacant(e),
            Entry::Occupied(e) => {
                e.node.value.as_mut().map(f);
                Entry::Occupied(e)
            }
        }
    }
}

impl<'a, P, T> Entry<'a, P, T>
where
    P: Prefix,
    T: Default,
{
    /// Ensures a value is in the entry by inserting the default value if empty, and returns a
    /// mutable reference to the value in the entry.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// # #[cfg(feature = "ipnet")]
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut pm: PrefixMap<ipnet::Ipv4Net, _> = PrefixMap::new();
    /// pm.insert("192.168.1.0/24".parse()?, 1);
    ///
    /// assert_eq!(pm.entry("192.168.1.0/24".parse()?).or_default(), &1);
    /// assert_eq!(pm.entry("192.168.2.0/24".parse()?).or_default(), &0);
    ///
    /// assert_eq!(pm.get(&"192.168.1.0/24".parse()?), Some(&1));
    /// assert_eq!(pm.get(&"192.168.2.0/24".parse()?), Some(&0));
    /// # Ok(())
    /// # }
    /// # #[cfg(not(feature = "ipnet"))]
    /// # fn main() {}
    /// ```
    #[allow(clippy::unwrap_or_default)]
    #[inline(always)]
    pub fn or_default(self) -> &'a mut T {
        self.or_insert_with(Default::default)
    }
}

impl<'a, P, T> VacantEntry<'a, P, T>
where
    P: Prefix,
{
    fn _insert(self, v: T) -> &'a mut Node<P, T> {
        match self.direction {
            DirectionForInsert::Reached => {
                // increment the count, as node.value will be `None`. We do it here as we borrow
                // `map` mutably in the next line.
                self.map.count += 1;
                let node = &mut self.map.table[self.idx];
                node.prefix = self.prefix;
                debug_assert!(node.value.is_none());
                node.value = Some(v);
                node
            }
            DirectionForInsert::NewLeaf { right } => {
                let new = self.map.new_node(self.prefix, Some(v));
                self.map.table.set_child(self.idx, new, right);
                &mut self.map.table[new]
            }
            DirectionForInsert::NewChild { right, child_right } => {
                let new = self.map.new_node(self.prefix, Some(v));
                let child = self.map.table.set_child(self.idx, new, right).unwrap();
                self.map.table.set_child(new, child, child_right);
                &mut self.map.table[new]
            }
            DirectionForInsert::NewBranch {
                branch_prefix,
                right,
                prefix_right,
            } => {
                let branch = self.map.new_node(branch_prefix, None);
                let new = self.map.new_node(self.prefix, Some(v));
                let child = self.map.table.set_child(self.idx, branch, right).unwrap();
                self.map.table.set_child(branch, new, prefix_right);
                self.map.table.set_child(branch, child, !prefix_right);
                &mut self.map.table[new]
            }
            DirectionForInsert::Enter { .. } => unreachable!(),
        }
    }
}

impl<P, T> OccupiedEntry<'_, P, T> {
    /// Gets a reference to the key in the entry. This is the key that is currently stored, and not
    /// the key that was used in the insert.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// use prefix_trie::map::Entry;
    /// # #[cfg(feature = "ipnet")]
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut pm: PrefixMap<ipnet::Ipv4Net, _> = PrefixMap::new();
    /// pm.insert("192.168.1.0/24".parse()?, 1);
    /// match pm.entry("192.168.1.1/24".parse()?) {
    ///     Entry::Occupied(e) => assert_eq!(e.key(), &"192.168.1.0/24".parse()?),
    ///     Entry::Vacant(_) => unreachable!(),
    /// }
    /// # Ok(())
    /// # }
    /// # #[cfg(not(feature = "ipnet"))]
    /// # fn main() {}
    /// ```
    pub fn key(&self) -> &P {
        &self.node.prefix
    }

    /// Gets a reference to the value in the entry.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// use prefix_trie::map::Entry;
    /// # #[cfg(feature = "ipnet")]
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///
    /// let mut pm: PrefixMap<ipnet::Ipv4Net, _> = PrefixMap::new();
    /// pm.insert("192.168.1.0/24".parse()?, 1);
    /// match pm.entry("192.168.1.0/24".parse()?) {
    ///     Entry::Occupied(e) => assert_eq!(e.get(), &1),
    ///     Entry::Vacant(_) => unreachable!(),
    /// }
    /// # Ok(())
    /// # }
    /// # #[cfg(not(feature = "ipnet"))]
    /// # fn main() {}
    /// ```
    pub fn get(&self) -> &T {
        self.node.value.as_ref().unwrap()
    }

    /// Gets a mutable reference to the value in the entry.
    ///
    /// This call will not modify the prefix stored in the tree. In case the prefix used to create
    /// the entry is different from the stored one (has additional information in the host part),
    /// and you wish that prefix to be overwritten, use `insert`.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// use prefix_trie::map::Entry;
    /// # #[cfg(feature = "ipnet")]
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///
    /// let mut pm: PrefixMap<ipnet::Ipv4Net, _> = PrefixMap::new();
    /// pm.insert("192.168.1.0/24".parse()?, 1);
    /// match pm.entry("192.168.1.0/24".parse()?) {
    ///     Entry::Occupied(mut e) => *e.get_mut() += 1,
    ///     Entry::Vacant(_) => unreachable!(),
    /// }
    /// assert_eq!(pm.get(&"192.168.1.0/24".parse()?), Some(&2));
    /// # Ok(())
    /// # }
    /// # #[cfg(not(feature = "ipnet"))]
    /// # fn main() {}
    /// ```
    pub fn get_mut(&mut self) -> &mut T {
        self.node.value.as_mut().unwrap()
    }

    /// Insert a new value into the entry, returning the old value. This operation will also replace
    /// the prefix with the provided one.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// use prefix_trie::map::Entry;
    /// # #[cfg(feature = "ipnet")]
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///
    /// let mut pm: PrefixMap<ipnet::Ipv4Net, _> = PrefixMap::new();
    /// pm.insert("192.168.1.0/24".parse()?, 1);
    /// match pm.entry("192.168.1.0/24".parse()?) {
    ///     Entry::Occupied(mut e) => assert_eq!(e.insert(10), 1),
    ///     Entry::Vacant(_) => unreachable!(),
    /// }
    /// assert_eq!(pm.get(&"192.168.1.0/24".parse()?), Some(&10));
    /// # Ok(())
    /// # }
    /// # #[cfg(not(feature = "ipnet"))]
    /// # fn main() {}
    /// ```
    pub fn insert(self, value: T) -> T {
        self.node.prefix = self.prefix;
        self.node.value.replace(value).unwrap()
    }

    /// Remove the current value and return it. The tree will not be modified (the same effect as
    /// `PrefixMap::remove_keep_tree`).
    ///
    /// ```
    /// # use prefix_trie::*;
    /// use prefix_trie::map::Entry;
    /// # #[cfg(feature = "ipnet")]
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///
    /// let mut pm: PrefixMap<ipnet::Ipv4Net, i32> = PrefixMap::new();
    /// pm.insert("192.168.1.0/24".parse()?, 1);
    /// match pm.entry("192.168.1.0/24".parse()?) {
    ///     Entry::Occupied(mut e) => assert_eq!(e.remove(), 1),
    ///     Entry::Vacant(_) => unreachable!(),
    /// }
    /// assert_eq!(pm.get(&"192.168.1.0/24".parse()?), None);
    /// # Ok(())
    /// # }
    /// # #[cfg(not(feature = "ipnet"))]
    /// # fn main() {}
    /// ```
    pub fn remove(&mut self) -> T {
        self.node.value.take().unwrap()
    }
}

impl<P, T> VacantEntry<'_, P, T> {
    /// Gets a reference to the key in the entry.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// use prefix_trie::map::Entry;
    /// # #[cfg(feature = "ipnet")]
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut pm: PrefixMap<ipnet::Ipv4Net, i32> = PrefixMap::new();
    /// match pm.entry("192.168.1.0/24".parse()?) {
    ///     Entry::Vacant(e) => assert_eq!(e.key(), &"192.168.1.0/24".parse()?),
    ///     Entry::Occupied(_) => unreachable!(),
    /// }
    /// # Ok(())
    /// # }
    /// # #[cfg(not(feature = "ipnet"))]
    /// # fn main() {}
    /// ```
    pub fn key(&self) -> &P {
        &self.prefix
    }
}

impl<'a, P, T> VacantEntry<'a, P, T>
where
    P: Prefix,
{
    /// Get a mutable reference to the value. If the value is yet empty, set it to the given default
    /// value.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// use prefix_trie::map::Entry;
    /// # #[cfg(feature = "ipnet")]
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut pm: PrefixMap<ipnet::Ipv4Net, i32> = PrefixMap::new();
    /// match pm.entry("192.168.1.0/24".parse()?) {
    ///     Entry::Vacant(mut e) => assert_eq!(e.insert(10), &10),
    ///     Entry::Occupied(_) => unreachable!(),
    /// }
    /// assert_eq!(pm.get(&"192.168.1.0/24".parse()?), Some(&10));
    /// # Ok(())
    /// # }
    /// # #[cfg(not(feature = "ipnet"))]
    /// # fn main() {}
    /// ```
    pub fn insert(self, default: T) -> &'a mut T {
        let node = self._insert(default);
        node.value.as_mut().unwrap()
    }

    /// Get a mutable reference to the value. If the value is yet empty, set it to the return value
    /// from the given function.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// use prefix_trie::map::Entry;
    /// # #[cfg(feature = "ipnet")]
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut pm: PrefixMap<ipnet::Ipv4Net, i32> = PrefixMap::new();
    /// match pm.entry("192.168.1.0/24".parse()?) {
    ///     Entry::Vacant(mut e) => assert_eq!(e.insert_with(|| 10), &10),
    ///     Entry::Occupied(_) => unreachable!(),
    /// }
    /// assert_eq!(pm.get(&"192.168.1.0/24".parse()?), Some(&10));
    /// # Ok(())
    /// # }
    /// # #[cfg(not(feature = "ipnet"))]
    /// # fn main() {}
    /// ```
    pub fn insert_with<F: FnOnce() -> T>(self, default: F) -> &'a mut T {
        let node = self._insert(default());
        node.value.as_mut().unwrap()
    }
}

impl<'a, P, T> VacantEntry<'a, P, T>
where
    P: Prefix,
    T: Default,
{
    /// Get a mutable reference to the value. If the value is yet empty, set it to the default value
    /// using `Default::default()`.
    ///
    /// ```
    /// # use prefix_trie::*;
    /// use prefix_trie::map::Entry;
    /// # #[cfg(feature = "ipnet")]
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut pm: PrefixMap<ipnet::Ipv4Net, i32> = PrefixMap::new();
    /// match pm.entry("192.168.1.0/24".parse()?) {
    ///     Entry::Vacant(e) => assert_eq!(e.default(), &0),
    ///     Entry::Occupied(_) => unreachable!(),
    /// }
    /// assert_eq!(pm.get(&"192.168.1.0/24".parse()?), Some(&0));
    /// # Ok(())
    /// # }
    /// # #[cfg(not(feature = "ipnet"))]
    /// # fn main() {}
    /// ```
    pub fn default(self) -> &'a mut T {
        let node = self._insert(Default::default());
        node.value.as_mut().unwrap()
    }
}
