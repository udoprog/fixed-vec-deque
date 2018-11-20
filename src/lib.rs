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

/// Code extensively based on Rust stdlib:
/// https://github.com/rust-lang/rust/blob/e8aef7cae14bc7a56859408c90253e9bcc07fcff/src/liballoc/collections/vec_deque.rs
/// And rust-smallvec:
/// https://github.com/servo/rust-smallvec
use std::cmp;
use std::fmt;
use std::hash;
use std::iter::{repeat, FromIterator};
use std::marker;
use std::mem;
use std::ops::{Index, IndexMut};
use std::ptr;
use std::slice;

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

impl<T> Clone for FixedVecDeque<T>
where
    T: Array,
{
    fn clone(&self) -> Self {
        FixedVecDeque {
            ptr: self.ptr,
            len: self.len,
            data: unsafe {
                let mut data: T = mem::uninitialized();
                ptr::copy_nonoverlapping(self.data.ptr(), data.ptr_mut(), T::size());
                data
            },
        }
    }
}

impl<T> FixedVecDeque<T>
where
    T: Array,
    T::Item: Default,
{
    /// Construct a new fixed ring buffer, pre-allocating all elements through [`Default`].
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use fixed_vec_deque::FixedVecDeque;
    ///
    /// let mut deq = FixedVecDeque::<[u32; 16]>::new();
    /// assert_eq!(deq, []);
    /// *deq.push_back() = 1;
    /// assert_eq!(deq, [1]);
    /// ```
    pub fn new() -> Self {
        FixedVecDeque {
            ptr: 0,
            len: 0,
            data: Self::data_from_default(),
        }
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

impl<T> FixedVecDeque<T>
where
    T: Array,
{
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

    /// Returns the number of elements the `FixedVecDeque` can hold.
    ///
    /// # Examples
    ///
    /// ```
    /// use fixed_vec_deque::FixedVecDeque;
    ///
    /// let buf = FixedVecDeque::<[u32; 16]>::new();
    /// assert_eq!(buf.capacity(), 16);
    /// ```
    #[inline]
    pub fn capacity(&self) -> usize {
        T::size()
    }

    /// Shortens the `FixedVecDeque`, causing excess elements to be unused.
    ///
    /// If `len` is greater than the `FixedVecDeque`'s current length, this has no
    /// effect.
    ///
    /// # Examples
    ///
    /// ```
    /// use fixed_vec_deque::FixedVecDeque;
    ///
    /// let mut buf = FixedVecDeque::<[u32; 4]>::new();
    /// *buf.push_back() = 5;
    /// *buf.push_back() = 10;
    /// *buf.push_back() = 15;
    /// assert_eq!(buf, [5, 10, 15]);
    /// buf.truncate(1);
    /// assert_eq!(buf, [5]);
    /// ```
    pub fn truncate(&mut self, len: usize) {
        if len < self.len {
            self.ptr = T::wrap_sub(self.ptr, self.len - len);
            self.len = len;
        }
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

        let front = self.tail();
        Some(unsafe { self.buffer(front) })
    }

    /// Provides a mutable reference to the front element, or `None` if the `FixedVecDeque` is
    /// empty.
    ///
    /// # Examples
    ///
    /// ```
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

        let front = self.tail();
        Some(unsafe { self.buffer_mut(front) })
    }

    /// Provides a reference to the back element, or `None` if the `FixedVecDeque` is
    /// empty.
    ///
    /// # Examples
    ///
    /// ```
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
        let front = self.tail();
        unsafe { self.buffer_mut(front) }
    }

    /// Removes the first element and returns it, or `None` if the `FixedVecDeque` is
    /// empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use fixed_vec_deque::FixedVecDeque;
    ///
    /// let mut d = FixedVecDeque::<[u32; 2]>::new();
    /// *d.push_back() = 1;
    /// *d.push_back() = 2;
    ///
    /// assert_eq!(d.pop_front(), Some(&mut 1));
    /// assert_eq!(d.pop_front(), Some(&mut 2));
    /// assert_eq!(d.pop_front(), None);
    /// ```
    pub fn pop_front(&mut self) -> Option<&mut T::Item> {
        if self.is_empty() {
            return None;
        }

        let tail = self.tail();
        self.len -= 1;
        unsafe { Some(self.buffer_mut(tail)) }
    }

    /// Appends an element to the back of the `FixedVecDeque` by returning a mutable reference that
    /// can be modified to it.
    ///
    /// Note: this might potentially remove elements from the head, unless they have been read.
    ///
    /// # Examples
    ///
    /// ```
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
    /// use fixed_vec_deque::FixedVecDeque;
    ///
    /// let mut buf = FixedVecDeque::<[u32; 2]>::new();
    /// assert_eq!(buf.pop_back(), None);
    /// *buf.push_back() = 1;
    /// *buf.push_back() = 3;
    /// assert_eq!(buf.pop_back(), Some(&mut 3));
    /// ```
    pub fn pop_back(&mut self) -> Option<&mut T::Item> {
        if self.is_empty() {
            return None;
        }

        self.ptr = T::wrap_sub(self.ptr, 1);
        self.len -= 1;
        let ptr = self.ptr;
        unsafe { Some(self.buffer_mut(ptr)) }
    }

    /// Removes an element from anywhere in the `FixedVecDeque` and returns a mutable reference to
    /// it, replacing it with the last element.
    ///
    /// This does not preserve ordering, but is O(1).
    ///
    /// Returns `None` if `index` is out of bounds.
    ///
    /// Element at index 0 is the front of the queue.
    ///
    /// # Examples
    ///
    /// ```
    /// use fixed_vec_deque::FixedVecDeque;
    ///
    /// let mut buf = FixedVecDeque::<[u32; 4]>::new();
    /// assert_eq!(buf.swap_remove_back(0), None);
    /// *buf.push_back() = 1;
    /// *buf.push_back() = 2;
    /// *buf.push_back() = 3;
    /// assert_eq!(buf, [1, 2, 3]);
    ///
    /// assert_eq!(buf.swap_remove_back(0), Some(&mut 1));
    /// assert_eq!(buf, [3, 2]);
    /// ```
    pub fn swap_remove_back(&mut self, index: usize) -> Option<&mut T::Item> {
        let length = self.len();
        if length > 0 && index < length - 1 {
            self.swap(index, length - 1);
        } else if index >= length {
            return None;
        }
        self.pop_back()
    }

    /// Removes an element from anywhere in the `FixedVecDeque` and returns a reference to it,
    /// replacing it with the first element.
    ///
    /// This does not preserve ordering, but is O(1).
    ///
    /// Returns `None` if `index` is out of bounds.
    ///
    /// Element at index 0 is the front of the queue.
    ///
    /// # Examples
    ///
    /// ```
    /// use fixed_vec_deque::FixedVecDeque;
    ///
    /// let mut buf = FixedVecDeque::<[u32; 4]>::new();
    /// assert_eq!(buf.swap_remove_front(0), None);
    /// *buf.push_back() = 1;
    /// *buf.push_back() = 2;
    /// *buf.push_back() = 3;
    /// assert_eq!(buf, [1, 2, 3]);
    ///
    /// assert_eq!(buf.swap_remove_front(2), Some(&mut 3));
    /// assert_eq!(buf, [2, 1]);
    /// ```
    pub fn swap_remove_front(&mut self, index: usize) -> Option<&mut T::Item> {
        let length = self.len();
        if length > 0 && index < length && index != 0 {
            self.swap(index, 0);
        } else if index >= length {
            return None;
        }
        self.pop_front()
    }

    /// Removes and returns the element at `index` from the `VecDeque`.
    /// Whichever end is closer to the removal point will be moved to make
    /// room, and all the affected elements will be moved to new positions.
    /// Returns `None` if `index` is out of bounds.
    ///
    /// Element at index 0 is the front of the queue.
    ///
    /// # Examples
    ///
    /// ```
    /// use fixed_vec_deque::FixedVecDeque;
    ///
    /// let mut buf = FixedVecDeque::<[u32; 4]>::new();
    /// *buf.push_back() = 1;
    /// *buf.push_back() = 2;
    /// *buf.push_back() = 3;
    /// assert_eq!(buf, [1, 2, 3]);
    ///
    /// assert_eq!(buf.remove(1), Some(&mut 2));
    /// assert_eq!(buf, [1, 3]);
    /// ```
    pub fn remove(&mut self, index: usize) -> Option<&mut T::Item>
    where
        T::Item: fmt::Debug,
    {
        // if empty, nothing to do.
        if T::size() == 0 || index >= self.len {
            return None;
        }

        // There are three main cases:
        //  Elements are contiguous
        //  Elements are discontiguous and the removal is in the tail section
        //  Elements are discontiguous and the removal is in the head section
        //      - special case when elements are technically contiguous,
        //        but self.head = 0
        //
        // For each of those there are two more cases:
        //  Insert is closer to tail
        //  Insert is closer to head
        //
        // Key: H - self.head
        //      T - self.tail
        //      o - Valid element
        //      x - Element marked for removal
        //      R - Indicates element that is being removed
        //      M - Indicates element was moved

        let idx = self.ptr_index(index);
        let head = self.ptr;
        let tail = self.tail();

        let tmp = unsafe { self.buffer_read(idx) };

        let distance_to_tail = index;
        let distance_to_head = self.len() - index;

        let contiguous = self.is_contiguous();

        let idx = match (
            contiguous,
            distance_to_tail <= distance_to_head,
            idx >= tail,
        ) {
            (true, true, _) => {
                unsafe {
                    // contiguous, remove closer to tail:
                    //
                    //             T   R         H
                    //      [. . . o o x o o o o . . . . . .]
                    //
                    //               T           H
                    //      [. . . . o o o o o o . . . . . .]
                    //               M M

                    self.copy(tail + 1, tail, index);
                    tail
                }
            }
            (true, false, _) => {
                unsafe {
                    // contiguous, remove closer to head:
                    //
                    //             T       R     H
                    //      [. . . o o o o x o o . . . . . .]
                    //
                    //             T           H
                    //      [. . . o o o o o o . . . . . . .]
                    //                     M M

                    self.copy(idx, idx + 1, head - idx - 1);
                    self.ptr -= 1;
                    head
                }
            }
            (false, true, true) => {
                unsafe {
                    // discontiguous, remove closer to tail, tail section:
                    //
                    //                   H         T   R
                    //      [o o o o o o . . . . . o o x o o]
                    //
                    //                   H           T
                    //      [o o o o o o . . . . . . o o o o]
                    //                               M M

                    self.copy(tail + 1, tail, index);
                    tail
                }
            }
            (false, false, false) => {
                unsafe {
                    // discontiguous, remove closer to head, head section:
                    //
                    //               R     H           T
                    //      [o o o o x o o . . . . . . o o o]
                    //
                    //                   H             T
                    //      [o o o o o o . . . . . . . o o o]
                    //               M M

                    self.copy(idx, idx + 1, head - idx - 1);
                    self.ptr -= 1;
                    head
                }
            }
            (false, false, true) => {
                unsafe {
                    // discontiguous, remove closer to head, tail section:
                    //
                    //             H           T         R
                    //      [o o o . . . . . . o o o o o x o]
                    //
                    //           H             T
                    //      [o o . . . . . . . o o o o o o o]
                    //       M M                         M M
                    //
                    // or quasi-discontiguous, remove next to head, tail section:
                    //
                    //       H                 T         R
                    //      [. . . . . . . . . o o o o o x o]
                    //
                    //                         T           H
                    //      [. . . . . . . . . o o o o o o .]
                    //                                   M

                    // draw in elements in the tail section
                    self.copy(idx, idx + 1, T::size() - idx - 1);

                    // Prevents underflow.
                    if head != 0 {
                        // copy first element into empty spot
                        self.copy(T::size() - 1, 0, 1);

                        // move elements in the head section backwards
                        self.copy(0, 1, head - 1);
                    }

                    self.ptr = T::wrap_sub(self.ptr, 1);
                    head
                }
            }
            (false, true, false) => {
                unsafe {
                    // discontiguous, remove closer to tail, head section:
                    //
                    //           R               H     T
                    //      [o o x o o o o o o o . . . o o o]
                    //
                    //                           H       T
                    //      [o o o o o o o o o o . . . . o o]
                    //       M M M                       M M

                    // draw in elements up to idx
                    self.copy(1, 0, idx);

                    // copy last element into empty spot
                    self.copy(0, T::size() - 1, 1);

                    // move elements from tail to end forward, excluding the last one
                    self.copy(tail + 1, tail, T::size() - tail - 1);

                    tail
                }
            }
        };

        self.len -= 1;

        unsafe {
            // write temporary into shifted location since we need a stable memory location for it!
            self.buffer_write(idx, tmp);
            Some(self.buffer_mut(idx))
        }
    }

    /// Retains only the elements specified by the predicate.
    ///
    /// In other words, remove all elements `e` such that `f(&e)` returns false.
    /// This method operates in place and preserves the order of the retained
    /// elements.
    ///
    /// # Examples
    ///
    /// ```
    /// use fixed_vec_deque::FixedVecDeque;
    ///
    /// let mut buf = FixedVecDeque::<[usize; 8]>::new();
    /// buf.extend(1..5);
    /// buf.retain(|&x| x % 2 == 0);
    /// assert_eq!(buf, [2, 4]);
    /// ```
    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(&T::Item) -> bool,
    {
        let len = self.len();
        let mut del = 0;

        for i in 0..len {
            let off = self.ptr_index(i);

            if !f(unsafe { self.buffer(off) }) {
                del += 1;
            } else if del > 0 {
                self.swap(i - del, i);
            }
        }

        if del > 0 {
            self.truncate(len - del);
        }
    }

    /// Returns a front-to-back iterator.
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

    /// Returns `true` if the `FixedVecDeque` contains an element equal to the
    /// given value.
    ///
    /// # Examples
    ///
    /// ```
    /// use fixed_vec_deque::FixedVecDeque;
    ///
    /// let mut vector = FixedVecDeque::<[u32; 4]>::new();
    ///
    /// *vector.push_back() = 0;
    /// *vector.push_back() = 1;
    ///
    /// assert_eq!(vector.contains(&1), true);
    /// assert_eq!(vector.contains(&10), false);
    /// ```
    pub fn contains(&self, x: &T::Item) -> bool
    where
        T::Item: PartialEq<T::Item>,
    {
        let (a, b) = self.as_slices();
        a.contains(x) || b.contains(x)
    }

    /// Returns a pair of slices which contain, in order, the contents of the `FixedVecDeque`.
    ///
    /// # Examples
    ///
    /// ```
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
        let tail = self.tail();
        let buf = unsafe { self.buffer_as_mut_slice() };
        RingSlices::ring_slices(buf, head, tail)
    }

    /// Returns a pair of slices which contain, in order, the contents of the `FixedVecDeque`.
    ///
    /// # Examples
    ///
    /// ```
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

    /// Retrieves an element in the `FixedVecDeque` by index.
    ///
    /// Element at index 0 is the front of the queue.
    ///
    /// # Examples
    ///
    /// ```
    /// use fixed_vec_deque::FixedVecDeque;
    ///
    /// let mut buf = FixedVecDeque::<[u32; 5]>::new();
    /// *buf.push_back() = 3;
    /// *buf.push_back() = 4;
    /// *buf.push_back() = 5;
    /// assert_eq!(buf.get(1), Some(&4));
    /// ```
    pub fn get(&self, index: usize) -> Option<&T::Item> {
        if index < self.len {
            let off = self.ptr_index(index);
            Some(unsafe { self.buffer(off) })
        } else {
            None
        }
    }

    /// Retrieves an element in the `FixedVecDeque` mutably by index.
    ///
    /// Element at index 0 is the front of the queue.
    ///
    /// # Examples
    ///
    /// ```
    /// use fixed_vec_deque::FixedVecDeque;
    ///
    /// let mut buf = FixedVecDeque::<[u32; 5]>::new();
    /// *buf.push_back() = 3;
    /// *buf.push_back() = 4;
    /// *buf.push_back() = 5;
    /// if let Some(elem) = buf.get_mut(1) {
    ///     *elem = 7;
    /// }
    ///
    /// assert_eq!(buf[1], 7);
    /// ```
    pub fn get_mut(&mut self, index: usize) -> Option<&mut T::Item> {
        if index < self.len {
            let off = self.ptr_index(index);
            Some(unsafe { self.buffer_mut(off) })
        } else {
            None
        }
    }

    /// Swaps elements at indices `i` and `j`.
    ///
    /// `i` and `j` may be equal.
    ///
    /// Element at index 0 is the front of the queue.
    ///
    /// # Panics
    ///
    /// Panics if either index is out of bounds.
    ///
    /// # Examples
    ///
    /// ```
    /// use fixed_vec_deque::FixedVecDeque;
    ///
    /// let mut buf = FixedVecDeque::<[u32; 4]>::new();
    /// *buf.push_back() = 3;
    /// *buf.push_back() = 4;
    /// *buf.push_back() = 5;
    /// assert_eq!(buf, [3, 4, 5]);
    /// buf.swap(0, 2);
    /// assert_eq!(buf, [5, 4, 3]);
    /// ```
    pub fn swap(&mut self, i: usize, j: usize) {
        assert!(i < T::size());
        assert!(j < T::size());
        let ri = self.ptr_index(i);
        let rj = self.ptr_index(j);
        let d = self.data.ptr_mut();
        unsafe { ptr::swap(d.add(ri), d.add(rj)) }
    }

    /// Turn `i`, which is a zero-based offset into a ptr index that wraps around the size of this
    /// container.
    #[inline]
    fn ptr_index(&self, i: usize) -> usize {
        T::wrap_add(self.tail(), i)
    }

    /// Get index of tail.
    #[inline]
    fn tail(&self) -> usize {
        T::wrap_sub(self.ptr, self.len)
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

    #[inline]
    unsafe fn buffer_read(&mut self, off: usize) -> T::Item {
        debug_assert!(off < T::size());
        ptr::read(self.data.ptr().add(off))
    }

    #[inline]
    unsafe fn buffer_write(&mut self, off: usize, data: T::Item) {
        debug_assert!(off < T::size());
        ptr::write(self.data.ptr_mut().add(off), data);
    }

    #[inline]
    fn is_contiguous(&self) -> bool {
        self.len != T::size() && self.tail() <= self.ptr
    }

    /// Copies a contiguous block of memory len long from src to dst
    #[inline]
    unsafe fn copy(&mut self, dst: usize, src: usize, len: usize) {
        debug_assert!(
            dst + len <= T::size(),
            "cpy dst={} src={} len={} cap={}",
            dst,
            src,
            len,
            T::size()
        );

        debug_assert!(
            src + len <= T::size(),
            "cpy dst={} src={} len={} cap={}",
            dst,
            src,
            len,
            T::size()
        );

        let m = self.data.ptr_mut();
        ptr::copy(m.add(src), m.add(dst), len);
    }
}

impl<T> FixedVecDeque<T>
where
    T: Array,
    T::Item: Clone,
{
    /// Modifies the `FixedVecDeque` in-place so that `len()` is equal to new_len,
    /// either by removing excess elements from the back or by appending clones of `value`
    /// to the back.
    ///
    /// # Panics
    ///
    /// Panics if `new_len` is longer than the [`capacity`] of this buffer.
    ///
    /// # Examples
    ///
    /// ```
    /// use fixed_vec_deque::FixedVecDeque;
    ///
    /// let mut buf = FixedVecDeque::<[u32; 8]>::new();
    /// *buf.push_back() = 5;
    /// *buf.push_back() = 10;
    /// *buf.push_back() = 15;
    /// assert_eq!(buf, [5, 10, 15]);
    ///
    /// buf.resize(2, 0);
    /// assert_eq!(buf, [5, 10]);
    ///
    /// buf.resize(5, 20);
    /// assert_eq!(buf, [5, 10, 20, 20, 20]);
    /// ```
    ///
    /// [`capacity`]: struct.FixedVecDeque.html#method.capacity
    pub fn resize(&mut self, new_len: usize, value: T::Item) {
        assert!(new_len < T::size(), "resize beyond capacity");

        let len = self.len();

        if new_len > len {
            self.extend(repeat(value).take(new_len - len))
        } else {
            self.truncate(new_len);
        }
    }
}

impl<A> hash::Hash for FixedVecDeque<A>
where
    A: Array,
    A::Item: hash::Hash,
{
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.len().hash(state);
        let (a, b) = self.as_slices();
        hash::Hash::hash_slice(a, state);
        hash::Hash::hash_slice(b, state);
    }
}

impl<T> Index<usize> for FixedVecDeque<T>
where
    T: Array,
{
    type Output = T::Item;

    fn index(&self, index: usize) -> &T::Item {
        self.get(index).expect("Out of bounds access")
    }
}

impl<T> IndexMut<usize> for FixedVecDeque<T>
where
    T: Array,
{
    fn index_mut(&mut self, index: usize) -> &mut T::Item {
        self.get_mut(index).expect("Out of bounds access")
    }
}

/// An iterator over the elements of a `FixedVecDeque`.
///
/// This `struct` is created by the [`iter`] method on [`FixedVecDeque`]. See its
/// documentation for more.
///
/// [`iter`]: struct.FixedVecDeque.html#method.iter
/// [`FixedVecDeque`]: struct.FixedVecDeque.html
pub struct Iter<'a, T: 'a>
where
    T: Array,
{
    data: *const T::Item,
    ptr: usize,
    len: usize,
    marker: marker::PhantomData<&'a ()>,
}

impl<'a, T: 'a> Iterator for Iter<'a, T>
where
    T: Array,
{
    type Item = &'a T::Item;

    fn next(&mut self) -> Option<Self::Item> {
        if self.len == 0 {
            return None;
        }

        let ptr = T::wrap_sub(self.ptr, self.len);
        self.len -= 1;
        Some(unsafe { &*self.data.add(ptr) })
    }
}

/// An iterator over the elements of a `FixedVecDeque`.
///
/// This `struct` is created by the [`iter`] method on [`FixedVecDeque`]. See its
/// documentation for more.
///
/// [`iter`]: struct.FixedVecDeque.html#method.iter
/// [`FixedVecDeque`]: struct.FixedVecDeque.html
pub struct IterMut<'a, T: 'a>
where
    T: Array,
{
    data: *mut T::Item,
    ptr: usize,
    len: usize,
    marker: marker::PhantomData<&'a ()>,
}

impl<'a, T: 'a> Iterator for IterMut<'a, T>
where
    T: Array,
{
    type Item = &'a mut T::Item;

    fn next(&mut self) -> Option<Self::Item> {
        if self.len == 0 {
            return None;
        }

        let ptr = T::wrap_sub(self.ptr, self.len);
        self.len -= 1;
        Some(unsafe { &mut *self.data.add(ptr) })
    }
}

impl<'a, T: 'a> IntoIterator for &'a FixedVecDeque<T>
where
    T: Array,
{
    type Item = &'a T::Item;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<A> Extend<A::Item> for FixedVecDeque<A>
where
    A: Array,
{
    fn extend<T: IntoIterator<Item = A::Item>>(&mut self, iter: T) {
        for elt in iter {
            *self.push_back() = elt;
        }
    }
}

impl<T> fmt::Debug for FixedVecDeque<T>
where
    T: Array,
    T::Item: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_list().entries(self).finish()
    }
}

impl<A> FromIterator<A::Item> for FixedVecDeque<A>
where
    A: Array,
    A::Item: Default,
{
    fn from_iter<T: IntoIterator<Item = A::Item>>(iter: T) -> FixedVecDeque<A> {
        let mut deq = FixedVecDeque::new();
        deq.extend(iter.into_iter());
        deq
    }
}

/// Types that can be used as the backing store for a FixedVecDeque.
pub unsafe trait Array {
    /// The type of the array's elements.
    type Item;
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

impl<A> Eq for FixedVecDeque<A>
where
    A: Array,
    A::Item: Eq,
{
}

impl<A, B> PartialEq<FixedVecDeque<B>> for FixedVecDeque<A>
where
    A: Array,
    B: Array,
    A::Item: PartialEq<B::Item>,
{
    fn eq(&self, other: &FixedVecDeque<B>) -> bool {
        if self.len() != other.len() {
            return false;
        }
        let (sa, sb) = self.as_slices();
        let (oa, ob) = other.as_slices();
        if sa.len() == oa.len() {
            sa == oa && sb == ob
        } else if sa.len() < oa.len() {
            // Always divisible in three sections, for example:
            // self:  [a b c|d e f]
            // other: [0 1 2 3|4 5]
            // front = 3, mid = 1,
            // [a b c] == [0 1 2] && [d] == [3] && [e f] == [4 5]
            let front = sa.len();
            let mid = oa.len() - front;

            let (oa_front, oa_mid) = oa.split_at(front);
            let (sb_mid, sb_back) = sb.split_at(mid);
            debug_assert_eq!(sa.len(), oa_front.len());
            debug_assert_eq!(sb_mid.len(), oa_mid.len());
            debug_assert_eq!(sb_back.len(), ob.len());
            sa == oa_front && sb_mid == oa_mid && sb_back == ob
        } else {
            let front = oa.len();
            let mid = sa.len() - front;

            let (sa_front, sa_mid) = sa.split_at(front);
            let (ob_mid, ob_back) = ob.split_at(mid);
            debug_assert_eq!(sa_front.len(), oa.len());
            debug_assert_eq!(sa_mid.len(), ob_mid.len());
            debug_assert_eq!(sb.len(), ob_back.len());
            sa_front == oa && sa_mid == ob_mid && sb == ob_back
        }
    }
}

macro_rules! __impl_slice_eq1 {
    ($Lhs: ty, $Rhs: ty) => {
        __impl_slice_eq1! { $Lhs, $Rhs, Sized }
    };
    ($Lhs: ty, $Rhs: ty, $Bound: ident) => {
        impl<'a, 'b, A, B> PartialEq<$Rhs> for $Lhs
        where
            A: Array,
            A::Item: $Bound + PartialEq<B>
        {
            fn eq(&self, other: &$Rhs) -> bool {
                if self.len() != other.len() {
                    return false;
                }
                let (sa, sb) = self.as_slices();
                let (oa, ob) = other[..].split_at(sa.len());
                sa == oa && sb == ob
            }
        }
    }
}

__impl_slice_eq1! { FixedVecDeque<A>, Vec<B> }
__impl_slice_eq1! { FixedVecDeque<A>, &'b [B] }
__impl_slice_eq1! { FixedVecDeque<A>, &'b mut [B] }

macro_rules! array_impls {
    ($($N: expr)+) => {
        $(
            __impl_slice_eq1! { FixedVecDeque<A>, [B; $N] }
            __impl_slice_eq1! { FixedVecDeque<A>, &'b [B; $N] }
            __impl_slice_eq1! { FixedVecDeque<A>, &'b mut [B; $N] }
        )+
    }
}

array_impls! {
     0  1  2  3  4  5  6  7  8  9
    10 11 12 13 14 15 16 17 18 19
    20 21 22 23 24 25 26 27 28 29
    30 31 32
}

impl<A> PartialOrd for FixedVecDeque<A>
where
    A: Array,
    A::Item: PartialOrd,
{
    fn partial_cmp(&self, other: &FixedVecDeque<A>) -> Option<cmp::Ordering> {
        self.iter().partial_cmp(other.iter())
    }
}

impl<A> Ord for FixedVecDeque<A>
where
    A: Array,
    A::Item: Ord,
{
    #[inline]
    fn cmp(&self, other: &FixedVecDeque<A>) -> cmp::Ordering {
        self.iter().cmp(other.iter())
    }
}

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
        T: Array,
        T::Item: Default,
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

        assert_eq!(Some(&mut Foo { data: 1 }), fixed.pop_front());
        assert_eq!(Some(&mut Foo { data: 2 }), fixed.pop_front());
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
                assert_eq!(buf.get(0), None);
                assert_eq!(buf.get_mut(0), None);

                for i in 1..($size + 1) {
                    *buf.push_back() = i;

                    assert_eq!(buf.front(), Some(&1));
                    assert_eq!(buf.back(), Some(&i));
                    assert_eq!(buf.get(0), Some(&1));
                    assert_eq!(buf.get(buf.len() - 1), Some(&i));
                    assert_eq!(buf[0], 1);
                    assert_eq!(buf[buf.len() - 1], i);
                }

                let mut buf = FixedVecDeque::<[u32; $size]>::new();

                assert_eq!(buf.back(), None);
                assert_eq!(buf.front(), None);
                assert_eq!(buf.get(0), None);
                assert_eq!(buf.get_mut(0), None);

                for i in 1..($size + 1) {
                    *buf.push_front() = i;

                    assert_eq!(buf.back(), Some(&1));
                    assert_eq!(buf.front(), Some(&i));
                    assert_eq!(buf.get(buf.len() - 1), Some(&1));
                    assert_eq!(buf.get(0), Some(&i));
                    assert_eq!(buf[buf.len() - 1], 1);
                    assert_eq!(buf[0], i);
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

    #[test]
    fn test_clone() {
        let a: FixedVecDeque<[u32; 4]> = vec![1, 2, 3, 4].into_iter().collect();
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn test_swap_front_back_remove() {
        fn test(back: bool) {
            let mut tester = FixedVecDeque::<[usize; 16]>::new();
            let usable_cap = tester.capacity();
            let final_len = usable_cap / 2;

            for len in 0..final_len {
                let expected: FixedVecDeque<[usize; 16]> = if back {
                    (0..len).collect()
                } else {
                    (0..len).rev().collect()
                };
                for tail_pos in 0..usable_cap {
                    tester.ptr = tail_pos;
                    tester.len = 0;

                    if back {
                        for i in 0..len * 2 {
                            *tester.push_front() = i;
                        }
                        for i in 0..len {
                            assert_eq!(tester.swap_remove_back(i), Some(&mut (len * 2 - 1 - i)));
                        }
                    } else {
                        for i in 0..len * 2 {
                            *tester.push_back() = i;
                        }
                        for i in 0..len {
                            let idx = tester.len() - 1 - i;
                            assert_eq!(tester.swap_remove_front(idx), Some(&mut (len * 2 - 1 - i)));
                        }
                    }
                    assert_eq!(tester, expected);
                }
            }
        }
        test(true);
        test(false);
    }

    #[test]
    fn test_basic_remove() {
        let mut a = FixedVecDeque::<[usize; 16]>::new();
        *a.push_front() = 2;
        *a.push_front() = 1;
        *a.push_back() = 3;
        *a.push_back() = 4;

        assert_eq!(a, [1, 2, 3, 4]);

        assert_eq!(a.remove(2), Some(&mut 3));
        assert_eq!(a, [1, 2, 4]);
        assert_eq!(a.remove(2), Some(&mut 4));
        assert_eq!(a, [1, 2]);
        assert_eq!(a.remove(0), Some(&mut 1));
        assert_eq!(a, [2]);
        assert_eq!(a.remove(0), Some(&mut 2));
        assert_eq!(a, []);
    }

    #[test]
    fn test_remove() {
        // This test checks that every single combination of tail position, length, and
        // removal position is tested. Capacity 15 should be large enough to cover every case.

        let mut tester = FixedVecDeque::<[usize; 16]>::new();

        // can't guarantee we got 15, so have to get what we got.
        // 15 would be great, but we will definitely get 2^k - 1, for k >= 4, or else
        // this test isn't covering what it wants to
        let cap = tester.capacity();

        // len is the length *after* removal
        for len in 0..cap - 1 {
            // 0, 1, 2, .., len - 1
            let expected = (0..).take(len).collect::<FixedVecDeque<[usize; 16]>>();
            for tail_pos in 0..cap {
                for to_remove in 0..len + 1 {
                    tester.ptr = tail_pos;
                    tester.len = 0;

                    for i in 0..len {
                        if i == to_remove {
                            *tester.push_back() = 1234;
                        }
                        *tester.push_back() = i;
                    }
                    if to_remove == len {
                        *tester.push_back() = 1234;
                    }
                    tester.remove(to_remove);
                    assert!(tester.tail() < tester.capacity());
                    assert!(tester.ptr < tester.capacity());
                    assert_eq!(tester, expected);
                }
            }
        }
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

            BigStruct { fields }
        }
    }
}
