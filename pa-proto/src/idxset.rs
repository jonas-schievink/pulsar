//! A container that associates incrementing IDs to the stored items.

use std::collections::{btree_map, BTreeMap};
use std::marker::PhantomData;
use std::cmp::Ordering;
use std::fmt;

/// An index for accessing a `T` stored in an `IdxSet`.
///
/// `Idx` is tagged with the type is refers to via its type parameter `T`, but doesn't contain a `T`
/// by itself (it's just a small index). This helps against accidental use of an `Idx` created by an
/// unrelated `IdxSet`.
pub struct Idx<T> {
    val: u32,
    phantom: PhantomData<T>,
}

impl<T> Idx<T> {
    /// Get the raw `u32` inside this `Idx`.
    pub fn value(&self) -> u32 {
        self.val
    }
}

impl<T> Clone for Idx<T> {
    fn clone(&self) -> Self {
        Self {
            val: self.val,
            phantom: PhantomData,
        }
    }
}

impl<T> Copy for Idx<T> {}

impl<T> PartialEq for Idx<T> {
    fn eq(&self, other: &Self) -> bool {
        self.val.eq(&other.val)
    }
}

impl<T> Eq for Idx<T> {}

impl<T> PartialOrd for Idx<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.val.partial_cmp(&other.val)
    }
}

impl<T> Ord for Idx<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.val.cmp(&other.val)
    }
}

impl<T> Into<u32> for Idx<T> {
    fn into(self) -> u32 {
        self.val
    }
}

impl<T> fmt::Debug for Idx<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_tuple("Idx")
            .field(&self.val)
            .finish()
    }
}

/// A container of `T`s, where each `T` is associated with an ID.
///
/// IDs (`Idx`) are local to the `IdxSet` that created them. An `IdxSet` will never associate
/// multiple items with the same `Idx`, even when items are removed (indices are monotonically
/// increasing).
#[derive(Debug)]
pub struct IdxSet<T> {
    map: BTreeMap<Idx<T>, T>,
    next_idx: u32,
}

impl<T> IdxSet<T> {
    /// Creates a new, empty `IdxSet`.
    pub fn new() -> Self {
        Self {
            map: BTreeMap::new(),
            next_idx: 0,
        }
    }

    /// Allocates a new `Idx` from the `IdxSet`.
    ///
    /// The returned `Idx` is not associated to any item that was ever part of the `IdxSet`.
    fn alloc_idx(&mut self) -> Idx<T> {
        self.next_idx += 1;
        Idx {
            val: self.next_idx - 1,
            phantom: PhantomData,
        }
    }

    /// Allocate an object in the `IdxSet` by calling a closure with a newly allocated `Idx`.
    pub fn alloc<F: FnOnce(Idx<T>) -> T>(&mut self, f: F) -> Entry<T> {
        let idx = self.alloc_idx();
        match self.map.entry(idx) {
            btree_map::Entry::Vacant(entry) => {
                Entry {
                    idx,
                    t: entry.insert(f(idx)),
                }
            }
            btree_map::Entry::Occupied(_) => {
                panic!("index {:?} already allocated", idx);
            }
        }
    }

    /// Looks up the value associated with `idx` and returns a reference to it.
    ///
    /// If the value has been removed from the `IdxSet`, returns `None`. If no value was ever
    /// associated with `idx`, also returns `None`.
    pub fn get(&self, idx: Idx<T>) -> Option<&T> {
        self.map.get(&idx)
    }

    pub fn get_mut(&mut self, idx: Idx<T>) -> Option<&mut T> {
        self.map.get_mut(&idx)
    }

    /// Removes the value associated with `idx`.
    pub fn remove(&mut self, idx: Idx<T>) -> Option<T> {
        self.map.remove(&idx)
    }

    /// Returns the number of values that are currently stored inside the `IdxSet`.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn iter<'a>(&'a self) -> Iter<'a, T> {
        Iter { inner: self.map.iter() }
    }
}

/// A key-value entry (or index-value entry) in an `IdxSet`.
#[derive(Debug)]
pub struct Entry<'a, T: 'a> {
    idx: Idx<T>,
    t: &'a T,
}

impl<'a, T: 'a> Entry<'a, T> {
    /// Get the index associated with the value.
    pub fn idx(&self) -> Idx<T> { self.idx }

    /// Get a reference to the value in the `IdxSet`.
    pub fn value(&self) -> &'a T { self.t }
}

#[derive(Debug)]
pub struct Iter<'a, T: 'a> {
    inner: btree_map::Iter<'a, Idx<T>, T>,
}

impl<'a, T: 'a> Iterator for Iter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<<Self as Iterator>::Item> {
        self.inner.next().map(|(_, v)| v)
    }
}

impl<'a, T: 'a> Clone for Iter<'a, T> {
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone() }
    }
}

// TODO tests
