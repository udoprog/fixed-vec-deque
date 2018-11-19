//! A double-ended queue implemented with a fixed ring buffer.
//!
//! This queue has `O(1)` amortized inserts and removals from both ends of the
//! container. It also has `O(1)` indexing like a vector. The contained elements
//! are not required to be copyable, and the queue will be sendable if the
//! contained type is sendable.
//!
//! The size of the `FixedVecDeque` must be completely specified at construction time, like this:
//!
//! ```rust
//! # extern crate fixed_vec_deque;
//! use fixed_vec_deque::FixedVecDeque;
//!
//! let _ = FixedVecDeque::<[Foo; 4]>::new();
//!
//! #[derive(Default)]
//! struct Foo;
//! ```
//!
//! Modifications can only happen _in-place_, this means that items stored in the queue must always
//! implement `Default`.
//!
//! [`push_back`] and [`push_front`] don't take an argument, instead they return a mutable
//! reference so that the newly inserted element is mutated in-place:
//!
//! ```rust
//! # extern crate fixed_vec_deque;
//! use fixed_vec_deque::FixedVecDeque;
//!
//! let mut buf = FixedVecDeque::<[Foo; 4]>::new();
//! buf.push_back().data = 42;
//!
//! #[derive(Default)]
//! struct Foo {
//!     data: u32,
//! }
//! ```
//!
//! On a similar note, [`pop_front`] and [`pop_back`] returns references instead of moving the
//! elements.
//!
//! A consequence of this is that this structure _never_ modifies the data it contains, even if it
//! has been _popped_.
//!
//! # When should I use `FixedVecDeque`?
//!
//! Generally when the following holds:
//!
//! * You have a maximum number of elements that you need to store for a short period of time.
//! * You only need to modify part of the element from the default when pushed.
//!
//! A conventional collection require you to write a "complete" element every time it is added to
//! it.
//! With `FixedVecDeque` we can instead modify the existing elements in place, and keep track of
//! how many such logical "additions" we have done.
//! For example:
//!
//! ```rust
//! # extern crate fixed_vec_deque;
//! use fixed_vec_deque::FixedVecDeque;
//! use std::collections::VecDeque;
//!
//! pub struct BigStruct {
//!     fields: [u64; 100],
//! }
//!
//! impl Default for BigStruct {
//!     fn default() -> Self {
//!         BigStruct {
//!             fields: [0u64; 100],
//!         }
//!     }
//! }
//!
//! let mut deq = FixedVecDeque::<[BigStruct; 0x100]>::new();
//!
//! for i in 0..100 {
//!     deq.push_back().fields[i] = i as u64;
//!
//!     let mut count = 0;
//!
//!     for big in &deq {
//!         count += 1;
//!         assert_eq!(big.fields[i], i as u64);
//!     }
//!
//!     assert_eq!(count, 1);
//!     deq.clear();
//! }
//!
//! deq.clear();
//!
//! // Note: modifications are still stored in the ring buffer and will be visible the next time we
//! // push to it unless we cleared it.
//! for i in 0..100 {
//!     assert_eq!(deq.push_back().fields[i], i as u64);
//!     deq.clear();
//! }
//! ```
//!
//! [`push_back`]: struct.FixedVecDeque.html#method.push_back
//! [`push_front`]: struct.FixedVecDeque.html#method.push_front
//! [`pop_back`]: struct.FixedVecDeque.html#method.pop_back
//! [`pop_front`]: struct.FixedVecDeque.html#method.pop_front

#![cfg_attr(feature = "unstable", feature(test))]

use std::mem;
use std::ptr;
use std::slice;
use std::marker;
use std::iter::FromIterator;

/// A double-ended queue implemented with a fixed buffer.
pub struct FixedVecDeque<T>
where
    T: Array,
{
    // where we are currently writing.
    ptr: usize,
    // how many valid elements we have in the queue.
    len: usize,
    // underlying array.
    data: T,
}

impl<T> FixedVecDeque<T>
where
    T: Array,
{
    /// Construct a new fixed ring buffer, pre-allocating all elements.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// # extern crate fixed_vec_deque;
    /// ```
    pub fn new() -> Self {
        FixedVecDeque {
            ptr: 0,
            len: 0,
            data: Self::data_from_default(),
        }
    }

    /// Returns `true` if the `FixedVecDeque` is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate fixed_vec_deque;
    /// use fixed_vec_deque::FixedVecDeque;
    ///
    /// let mut v = FixedVecDeque::<[u32; 1]>::new();
    /// assert!(v.is_empty());
    /// *v.push_front() = 1;
    /// assert!(!v.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns `true` if the `FixedVecDeque` is full.
    ///
    /// Writing to a queue that is full will overwrite existing elements.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate fixed_vec_deque;
    /// use fixed_vec_deque::FixedVecDeque;
    ///
    /// let mut v = FixedVecDeque::<[u32; 1]>::new();
    /// assert!(!v.is_full());
    /// *v.push_front() = 1;
    /// assert!(v.is_full());
    /// ```
    pub fn is_full(&self) -> bool {
        self.len == T::size()
    }

    /// Returns the number of elements in the `FixedVecDeque`.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate fixed_vec_deque;
    /// use fixed_vec_deque::FixedVecDeque;
    ///
    /// let mut v = FixedVecDeque::<[u32; 2]>::new();
    /// assert_eq!(v.len(), 0);
    /// *v.push_back() = 1;
    /// assert_eq!(v.len(), 1);
    /// *v.push_back() = 1;
    /// assert_eq!(v.len(), 2);
    /// ```
    pub fn len(&self) -> usize {
        self.len
    }

    /// Provides a reference to the front element, or `None` if the `FixedVecDeque` is
    /// empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use fixed_vec_deque::FixedVecDeque;
    ///
    /// let mut d = FixedVecDeque::<[u32; 2]>::new();
    /// assert_eq!(d.front(), None);
    ///
    /// *d.push_back() = 1;
    /// *d.push_back() = 2;
    /// assert_eq!(d.front(), Some(&1));
    /// ```
    pub fn front(&self) -> Option<&T::Item> {
        if self.is_empty() {
            return None;
        }

        let front = T::wrap_sub(self.ptr, self.len);
        Some(unsafe { self.buffer(front) })
    }

    /// Provides a mutable reference to the front element, or `None` if the `FixedVecDeque` is
    /// empty.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate fixed_vec_deque;
    /// use fixed_vec_deque::FixedVecDeque;
    ///
    /// let mut d = FixedVecDeque::<[u32; 2]>::new();
    ///
    /// assert_eq!(d.front_mut(), None);
    ///
    /// *d.push_back() = 1;
    /// *d.push_back() = 2;
    ///
    /// match d.front_mut() {
    ///     Some(x) => *x = 9,
    ///     None => (),
    /// }
    ///
    /// assert_eq!(d.front(), Some(&9));
    /// assert_eq!(d.back(), Some(&2));
    /// ```
    pub fn front_mut(&mut self) -> Option<&mut T::Item> {
        if self.is_empty() {
            return None;
        }

        let front = T::wrap_sub(self.ptr, self.len);
        Some(unsafe { self.buffer_mut(front) })
    }

    /// Provides a reference to the back element, or `None` if the `FixedVecDeque` is
    /// empty.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate fixed_vec_deque;
    /// use fixed_vec_deque::FixedVecDeque;
    ///
    /// let mut d = FixedVecDeque::<[u32; 2]>::new();
    ///
    /// assert_eq!(d.back(), None);
    ///
    /// *d.push_back() = 1;
    /// *d.push_back() = 2;
    /// assert_eq!(d.back(), Some(&2));
    /// ```
    pub fn back(&self) -> Option<&T::Item> {
        if self.is_empty() {
            return None;
        }

        let back = T::wrap_sub(self.ptr, 1);
        Some(unsafe { self.buffer(back) })
    }

    /// Provides a mutable reference to the back element, or `None` if the
    /// `FixedVecDeque` is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate fixed_vec_deque;
    /// use fixed_vec_deque::FixedVecDeque;
    ///
    /// let mut d = FixedVecDeque::<[u32; 2]>::new();
    ///
    /// assert_eq!(d.back(), None);
    ///
    /// *d.push_back() = 1;
    /// *d.push_back() = 2;
    ///
    /// match d.back_mut() {
    ///     Some(x) => *x = 9,
    ///     None => (),
    /// }
    /// assert_eq!(d.back(), Some(&9));
    /// ```
    pub fn back_mut(&mut self) -> Option<&mut T::Item> {
        if self.is_empty() {
            return None;
        }

        let back = T::wrap_sub(self.ptr, 1);
        Some(unsafe { self.buffer_mut(back) })
    }

    /// Prepends an element to the `FixedVecDeque`.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate fixed_vec_deque;
    /// use fixed_vec_deque::FixedVecDeque;
    ///
    /// let mut d = FixedVecDeque::<[u32; 3]>::new();
    ///
    /// assert_eq!(d.front(), None);
    /// assert_eq!(d.back(), None);
    ///
    /// *d.push_front() = 1;
    /// assert_eq!(d.front(), Some(&1));
    /// assert_eq!(d.back(), Some(&1));
    ///
    /// *d.push_front() = 2;
    /// assert_eq!(d.front(), Some(&2));
    /// assert_eq!(d.back(), Some(&1));
    ///
    /// *d.push_front() = 3;
    /// assert_eq!(d.front(), Some(&3));
    /// assert_eq!(d.back(), Some(&1));
    ///
    /// *d.push_front() = 4;
    /// assert_eq!(d.front(), Some(&4));
    /// assert_eq!(d.back(), Some(&2));
    /// ```
    pub fn push_front(&mut self) -> &mut T::Item {
        // overwriting existing elements.
        if self.len == T::size() {
            self.ptr = T::wrap_sub(self.ptr, 1);
            let front = self.ptr;
            return unsafe { self.buffer_mut(front) };
        }

        self.len += 1;
        let front = T::wrap_sub(self.ptr, self.len);
        unsafe { self.buffer_mut(front) }
    }

    /// Removes the first element and returns it, or `None` if the `FixedVecDeque` is
    /// empty.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate fixed_vec_deque;
    /// use fixed_vec_deque::FixedVecDeque;
    ///
    /// let mut d = FixedVecDeque::<[u32; 2]>::new();
    /// *d.push_back() = 1;
    /// *d.push_back() = 2;
    ///
    /// assert_eq!(d.pop_front(), Some(&1));
    /// assert_eq!(d.pop_front(), Some(&2));
    /// assert_eq!(d.pop_front(), None);
    /// ```
    pub fn pop_front(&mut self) -> Option<&T::Item> {
        if self.is_empty() {
            return None;
        }

        let tail = T::wrap_sub(self.ptr, self.len);
        self.len -= 1;
        unsafe { Some(self.buffer(tail)) }
    }

    /// Appends an element to the back of the `FixedVecDeque` by returning a mutable reference that
    /// can be modified to it.
    ///
    /// Note: this might potentially remove elements from the head, unless they have been read.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate fixed_vec_deque;
    /// use fixed_vec_deque::FixedVecDeque;
    ///
    /// let mut buf = FixedVecDeque::<[u32; 2]>::new();
    /// assert_eq!(buf.back(), None);
    /// assert_eq!(buf.front(), None);
    ///
    /// *buf.push_back() = 1;
    ///
    /// assert_eq!(buf.front(), Some(&1));
    /// assert_eq!(buf.back(), Some(&1));
    ///
    /// *buf.push_back() = 2;
    ///
    /// assert_eq!(buf.front(), Some(&1));
    /// assert_eq!(buf.back(), Some(&2));
    ///
    /// *buf.push_back() = 3;
    ///
    /// assert_eq!(buf.front(), Some(&2));
    /// assert_eq!(buf.back(), Some(&3));
    /// ```
    ///
    /// ```
    /// # extern crate fixed_vec_deque;
    /// use fixed_vec_deque::FixedVecDeque;
    ///
    /// let mut buf = FixedVecDeque::<[u32; 1]>::new();
    /// assert_eq!(buf.back(), None);
    /// assert_eq!(buf.front(), None);
    ///
    /// *buf.push_back() = 1;
    ///
    /// assert_eq!(buf.front(), Some(&1));
    /// assert_eq!(buf.back(), Some(&1));
    ///
    /// *buf.push_back() = 2;
    ///
    /// assert_eq!(buf.front(), Some(&2));
    /// assert_eq!(buf.back(), Some(&2));
    ///
    /// buf.pop_back();
    ///
    /// assert!(buf.is_empty());
    /// assert_eq!(buf.back(), None);
    /// assert_eq!(buf.front(), None);
    /// ```
    pub fn push_back(&mut self) -> &mut T::Item {
        let head = self.ptr;
        self.ptr = T::wrap_add(self.ptr, 1);

        if self.len < T::size() {
            self.len += 1;
        }

        unsafe { self.buffer_mut(head) }
    }

    /// Removes the last element from the `FixedVecDeque` and returns a reference to it, or `None`
    /// if it is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate fixed_vec_deque;
    /// use fixed_vec_deque::FixedVecDeque;
    ///
    /// let mut buf = FixedVecDeque::<[u32; 2]>::new();
    /// assert_eq!(buf.pop_back(), None);
    /// *buf.push_back() = 1;
    /// *buf.push_back() = 3;
    /// assert_eq!(buf.pop_back(), Some(&3));
    /// ```
    pub fn pop_back(&mut self) -> Option<&T::Item> {
        if self.is_empty() {
            return None;
        }

        self.ptr = T::wrap_sub(self.ptr, 1);
        self.len -= 1;
        unsafe { Some(self.buffer(self.ptr)) }
    }

    /// Returns a front-to-back iterator.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate fixed_vec_deque;
    /// use fixed_vec_deque::FixedVecDeque;
    ///
    /// let mut buf = FixedVecDeque::<[u32; 4]>::new();
    /// *buf.push_back() = 5;
    /// *buf.push_back() = 3;
    /// *buf.push_back() = 4;
    ///
    /// let b: &[_] = &[&5, &3, &4];
    /// let c: Vec<&u32> = buf.iter().collect();
    /// assert_eq!(&c[..], b);
    /// ```
    pub fn iter<'a>(&'a self) -> Iter<'a, T> {
        Iter {
            data: self.data.ptr(),
            ptr: self.ptr,
            len: self.len,
            marker: marker::PhantomData,
        }
    }

    /// Returns a front-to-back iterator that returns mutable references.
    ///
    /// # Examples
    ///
    /// ```
    /// use fixed_vec_deque::FixedVecDeque;
    ///
    /// let mut buf = FixedVecDeque::<[u32; 4]>::new();
    /// *buf.push_back() = 5;
    /// *buf.push_back() = 3;
    /// *buf.push_back() = 4;
    /// for num in buf.iter_mut() {
    ///     *num = *num - 2;
    /// }
    /// let b: &[_] = &[&mut 3, &mut 1, &mut 2];
    /// assert_eq!(&buf.iter_mut().collect::<Vec<&mut u32>>()[..], b);
    /// ```
    pub fn iter_mut<'a>(&'a mut self) -> IterMut<'a, T> {
        IterMut {
            data: self.data.ptr_mut(),
            ptr: self.ptr,
            len: self.len,
            marker: marker::PhantomData,
        }
    }

    /// Clears the `FixedVecDeque`.
    ///
    /// The stored values will _not_ be deleted.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate fixed_vec_deque;
    /// use fixed_vec_deque::FixedVecDeque;
    ///
    /// let mut v = FixedVecDeque::<[u32; 1]>::new();
    /// *v.push_back() = 1;
    /// v.clear();
    /// assert!(v.is_empty());
    /// ```
    #[inline]
    pub fn clear(&mut self) {
        self.ptr = 0;
        self.len = 0;
    }

    /// Returns a pair of slices which contain, in order, the contents of the `FixedVecDeque`.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate fixed_vec_deque;
    /// use fixed_vec_deque::FixedVecDeque;
    ///
    /// let mut vector = FixedVecDeque::<[u32; 6]>::new();
    ///
    /// *vector.push_back() = 0;
    /// *vector.push_back() = 1;
    ///
    /// *vector.push_front() = 10;
    /// *vector.push_front() = 9;
    ///
    /// vector.as_mut_slices().0[0] = 42;
    /// vector.as_mut_slices().1[0] = 24;
    ///
    /// assert_eq!(vector.as_slices(), (&[42, 10][..], &[24, 1][..]));
    /// ```
    #[inline]
    pub fn as_mut_slices(&mut self) -> (&mut [T::Item], &mut [T::Item]) {
        if self.is_full() {
            let ptr = self.ptr;
            let buf = unsafe { self.buffer_as_mut_slice() };
            let (left, right) = buf.split_at(ptr);
            return (right, left);
        }

        let head = self.ptr;
        let tail = T::wrap_sub(self.ptr, self.len);
        let buf = unsafe { self.buffer_as_mut_slice() };
        RingSlices::ring_slices(buf, head, tail)
    }

    /// Returns a pair of slices which contain, in order, the contents of the `FixedVecDeque`.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate fixed_vec_deque;
    /// use fixed_vec_deque::FixedVecDeque;
    ///
    /// let mut vector = FixedVecDeque::<[u32; 5]>::new();
    ///
    /// *vector.push_back() = 1;
    /// *vector.push_back() = 2;
    /// *vector.push_back() = 3;
    ///
    /// assert_eq!(vector.as_slices(), (&[1, 2, 3][..], &[][..]));
    ///
    /// *vector.push_front() = 4;
    /// *vector.push_front() = 5;
    ///
    /// assert_eq!(vector.as_slices(), (&[5, 4][..], &[1, 2, 3][..]));
    /// ```
    #[inline]
    pub fn as_slices(&self) -> (&[T::Item], &[T::Item]) {
        let buf = unsafe { self.buffer_as_slice() };

        if self.len == T::size() {
            let (left, right) = buf.split_at(self.ptr);
            return (right, left);
        }

        let head = self.ptr;
        let tail = T::wrap_sub(head, self.len);
        RingSlices::ring_slices(buf, head, tail)
    }

    /// Turn ptr into a slice
    #[inline]
    unsafe fn buffer_as_slice(&self) -> &[T::Item] {
        slice::from_raw_parts(self.data.ptr(), T::size())
    }

    /// Turn ptr into a mut slice
    #[inline]
    unsafe fn buffer_as_mut_slice(&mut self) -> &mut [T::Item] {
        slice::from_raw_parts_mut(self.data.ptr_mut(), T::size())
    }

    /// Takes a reference of a value from the buffer.
    #[inline]
    unsafe fn buffer(&self, off: usize) -> &T::Item {
        &*self.data.ptr().add(off)
    }

    /// Takes a mutable reference of a value from the buffer.
    #[inline]
    unsafe fn buffer_mut<'a>(&'a mut self, off: usize) -> &'a mut T::Item {
        &mut *self.data.ptr_mut().add(off)
    }

    /// Initialize stored data using `Default::default()`
    fn data_from_default() -> T {
        unsafe {
            let mut data: T = mem::uninitialized();
            let ptr = data.ptr_mut();

            for o in 0..T::size() {
                ptr::write(ptr.add(o), T::Item::default());
            }

            data
        }
    }
}

/// An iterator over the elements of a `FixedVecDeque`.
///
/// This `struct` is created by the [`iter`] method on [`FixedVecDeque`]. See its
/// documentation for more.
///
/// [`iter`]: struct.FixedVecDeque.html#method.iter
/// [`FixedVecDeque`]: struct.FixedVecDeque.html
pub struct Iter<'a, T: 'a> where T: Array {
    data: *const T::Item,
    ptr: usize,
    len: usize,
    marker: marker::PhantomData<&'a ()>,
}

impl<'a, T: 'a> Iterator for Iter<'a, T>
    where T: Array
{
    type Item = &'a T::Item;

    fn next(&mut self) -> Option<Self::Item> {
        if self.len == 0 {
            return None;
        }

        let ptr = T::wrap_sub(self.ptr, self.len);
        self.len -= 1;
        Some(unsafe { &* self.data.add(ptr) })
    }
}

/// An iterator over the elements of a `FixedVecDeque`.
///
/// This `struct` is created by the [`iter`] method on [`FixedVecDeque`]. See its
/// documentation for more.
///
/// [`iter`]: struct.FixedVecDeque.html#method.iter
/// [`FixedVecDeque`]: struct.FixedVecDeque.html
pub struct IterMut<'a, T: 'a> where T: Array {
    data: *mut T::Item,
    ptr: usize,
    len: usize,
    marker: marker::PhantomData<&'a ()>,
}

impl<'a, T: 'a> Iterator for IterMut<'a, T>
    where T: Array
{
    type Item = &'a mut T::Item;

    fn next(&mut self) -> Option<Self::Item> {
        if self.len == 0 {
            return None;
        }

        let ptr = T::wrap_sub(self.ptr, self.len);
        self.len -= 1;
        Some(unsafe { &mut * self.data.add(ptr) })
    }
}

impl<'a, T: 'a> IntoIterator for &'a FixedVecDeque<T> where T: Array {
    type Item = &'a T::Item;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<A> Extend<A::Item> for FixedVecDeque<A> where A: Array {
    fn extend<T: IntoIterator<Item = A::Item>>(&mut self, iter: T) {
        for elt in iter {
            *self.push_back() = elt;
        }
    }
}

impl<A> FromIterator<A::Item> for FixedVecDeque<A> where A: Array {
    fn from_iter<T: IntoIterator<Item = A::Item>>(iter: T) -> FixedVecDeque<A> {
        let mut deq = FixedVecDeque::new();
        deq.extend(iter.into_iter());
        deq
    }
}

/// Types that can be used as the backing store for a FixedVecDeque.
pub unsafe trait Array {
    /// The type of the array's elements.
    type Item: Default;
    /// Returns the number of items the array can hold.
    fn size() -> usize;
    /// Returns a pointer to the first element of the array.
    fn ptr(&self) -> *const Self::Item;
    /// Returns a mutable pointer to the first element of the array.
    fn ptr_mut(&mut self) -> *mut Self::Item;

    /// Returns the index in the underlying buffer for a given logical element
    /// index + addend.
    #[inline]
    fn wrap_add(idx: usize, addend: usize) -> usize {
        (idx + addend) % Self::size()
    }

    /// Returns the index in the underlying buffer for a given logical element
    /// index - subtrahend.
    #[inline]
    fn wrap_sub(idx: usize, subtrahend: usize) -> usize {
        if subtrahend > idx {
            Self::size() - (subtrahend - idx)
        } else {
            idx - subtrahend
        }
    }
}

macro_rules! impl_array(
    ($($size:expr),+) => {
        $(
            unsafe impl<T> Array for [T; $size] where T: Default {
                type Item = T;
                fn size() -> usize { $size }
                fn ptr(&self) -> *const T { self.as_ptr() }
                fn ptr_mut(&mut self) -> *mut T { self.as_mut_ptr() }
            }
        )+
    }
);

impl_array!(
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 20, 24, 32, 36, 0x40, 0x80, 0x100,
    0x200, 0x400, 0x800, 0x1000, 0x2000, 0x4000, 0x8000, 0x10000, 0x20000, 0x40000, 0x80000,
    0x100000
);

/// Returns the two slices that cover the `FixedVecDeque`'s valid range
trait RingSlices: Sized {
    fn slice(self, from: usize, to: usize) -> Self;
    fn split_at(self, i: usize) -> (Self, Self);

    fn ring_slices(buf: Self, head: usize, tail: usize) -> (Self, Self) {
        let contiguous = tail <= head;
        if contiguous {
            let (empty, buf) = buf.split_at(0);
            (buf.slice(tail, head), empty)
        } else {
            let (mid, right) = buf.split_at(tail);
            let (left, _) = mid.split_at(head);
            (right, left)
        }
    }
}

impl<'a, T> RingSlices for &'a [T] {
    fn slice(self, from: usize, to: usize) -> Self {
        &self[from..to]
    }

    fn split_at(self, i: usize) -> (Self, Self) {
        (*self).split_at(i)
    }
}

impl<'a, T> RingSlices for &'a mut [T] {
    fn slice(self, from: usize, to: usize) -> Self {
        &mut self[from..to]
    }

    fn split_at(self, i: usize) -> (Self, Self) {
        (*self).split_at_mut(i)
    }
}

#[cfg(test)]
mod tests {
    use super::{Array, FixedVecDeque};
    use std::mem;

    /// Construct a new and verify that its size is the sum of all it's elements.
    fn test_new<T>() -> FixedVecDeque<T>
    where
        T: Array + Default,
    {
        let fixed = FixedVecDeque::<T>::new();

        assert_eq!(
            mem::size_of::<T::Item>() * 4 + mem::size_of::<FixedVecDeque<[Zero; 1]>>(),
            mem::size_of::<FixedVecDeque<[T::Item; 4]>>()
        );

        #[derive(Debug, Default, PartialEq, Eq)]
        struct Zero {}

        fixed
    }

    #[test]
    fn test_push_back() {
        let mut fixed = test_new::<[Foo; 4]>();

        #[derive(Debug, Default, PartialEq, Eq)]
        struct Foo {
            data: u64,
        }

        fixed.push_back().data = 1;
        fixed.push_back().data = 2;

        assert_eq!(Some(&Foo { data: 1 }), fixed.pop_front());
        assert_eq!(Some(&Foo { data: 2 }), fixed.pop_front());
        assert_eq!(None, fixed.pop_front());
    }

    // make sure that we correctly ported the various functions, since they depended on sizes being
    // aligned to a power of two.
    #[test]
    fn test_unaligned_sizes() {
        macro_rules! test_size {
            ($size:expr) => {
                let mut buf = FixedVecDeque::<[u32; $size]>::new();

                assert_eq!(buf.back(), None);
                assert_eq!(buf.front(), None);

                for i in 1..($size + 1) {
                    *buf.push_back() = i;

                    assert_eq!(buf.front(), Some(&1));
                    assert_eq!(buf.back(), Some(&i));
                }

                let mut buf = FixedVecDeque::<[u32; $size]>::new();

                assert_eq!(buf.back(), None);
                assert_eq!(buf.front(), None);

                for i in 1..($size + 1) {
                    *buf.push_front() = i;

                    assert_eq!(buf.back(), Some(&1));
                    assert_eq!(buf.front(), Some(&i));
                }
            };
        }

        test_size!(0);
        test_size!(1);
        test_size!(2);
        test_size!(3);
        test_size!(4);
        test_size!(5);
        test_size!(6);
        test_size!(7);
        test_size!(8);
        test_size!(9);
        test_size!(10);
        test_size!(11);
        test_size!(12);
        test_size!(13);
        test_size!(14);
        test_size!(15);
        test_size!(16);
        test_size!(20);
        test_size!(24);
        test_size!(32);
        test_size!(36);
    }

    #[test]
    fn test_drop() {
        let mut a = 0;
        let mut b = 0;
        let mut c = 0;

        {
            let mut fixed = FixedVecDeque::<[Foo; 2]>::new();
            fixed.push_back().value = Some(&mut a);
            fixed.push_back().value = Some(&mut b);
            fixed.push_back().value = Some(&mut c);
        }

        // NB: zero because it will have been overwritten due to the circular nature of the buffer.
        assert_eq!(a, 0);
        assert_eq!(b, 1);
        assert_eq!(c, 1);

        #[derive(Default)]
        struct Foo<'a> {
            value: Option<&'a mut u32>,
        }

        impl<'a> Drop for Foo<'a> {
            fn drop(&mut self) {
                if let Some(v) = self.value.take() {
                    *v += 1;
                }
            }
        }
    }

    #[test]
    fn test_extend() {
        let mut deq = FixedVecDeque::<[u32; 4]>::new();
        deq.extend(vec![1, 2, 3, 4, 5, 6, 7, 8].into_iter());

        assert!(!deq.is_empty());
        assert!(deq.is_full());
        assert_eq!(deq.iter().collect::<Vec<_>>(), vec![&5, &6, &7, &8]);
    }

    #[test]
    fn test_collect() {
        let deq: FixedVecDeque<[u32; 4]> = vec![1, 2, 3, 4, 5, 6, 7, 8].into_iter().collect();

        assert!(!deq.is_empty());
        assert!(deq.is_full());
        assert_eq!(deq.iter().collect::<Vec<_>>(), vec![&5, &6, &7, &8]);
    }
}

#[cfg(all(feature = "unstable", test))]
mod benches {
    extern crate test;

    use super::FixedVecDeque;

    #[bench]
    fn bench_push_back_100(b: &mut test::Bencher) {
        let mut deq = FixedVecDeque::<[BigStruct; 0x100]>::new();

        b.iter(|| {
            for i in 0..100 {
                let big = deq.push_back();
                big.fields[0] = i;
            }

            deq.clear();
        })
    }

    #[bench]
    fn bench_push_back_100_vec_deque(b: &mut test::Bencher) {
        use std::collections::VecDeque;

        let mut deq = VecDeque::new();

        b.iter(|| {
            for i in 0..100 {
                let mut big = BigStruct::default();
                big.fields[0] = i;
                deq.push_back(big);
            }

            deq.clear();
        })
    }

    pub struct BigStruct {
        fields: [u64; 64],
    }

    impl Default for BigStruct {
        fn default() -> Self {
            let fields = [0u64; 64];

            BigStruct {
                fields,
            }
        }
    }
}
