# fixed-vec-deque

[<img alt="github" src="https://img.shields.io/badge/github-udoprog/fixed--vec--deque-8da0cb?style=for-the-badge&logo=github" height="20">](https://github.com/udoprog/fixed-vec-deque)
[<img alt="crates.io" src="https://img.shields.io/crates/v/fixed-vec-deque.svg?style=for-the-badge&color=fc8d62&logo=rust" height="20">](https://crates.io/crates/fixed-vec-deque)
[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-fixed--vec--deque-66c2a5?style=for-the-badge&logoColor=white&logo=data:image/svg+xml;base64,PHN2ZyByb2xlPSJpbWciIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyIgdmlld0JveD0iMCAwIDUxMiA1MTIiPjxwYXRoIGZpbGw9IiNmNWY1ZjUiIGQ9Ik00ODguNiAyNTAuMkwzOTIgMjE0VjEwNS41YzAtMTUtOS4zLTI4LjQtMjMuNC0zMy43bC0xMDAtMzcuNWMtOC4xLTMuMS0xNy4xLTMuMS0yNS4zIDBsLTEwMCAzNy41Yy0xNC4xIDUuMy0yMy40IDE4LjctMjMuNCAzMy43VjIxNGwtOTYuNiAzNi4yQzkuMyAyNTUuNSAwIDI2OC45IDAgMjgzLjlWMzk0YzAgMTMuNiA3LjcgMjYuMSAxOS45IDMyLjJsMTAwIDUwYzEwLjEgNS4xIDIyLjEgNS4xIDMyLjIgMGwxMDMuOS01MiAxMDMuOSA1MmMxMC4xIDUuMSAyMi4xIDUuMSAzMi4yIDBsMTAwLTUwYzEyLjItNi4xIDE5LjktMTguNiAxOS45LTMyLjJWMjgzLjljMC0xNS05LjMtMjguNC0yMy40LTMzLjd6TTM1OCAyMTQuOGwtODUgMzEuOXYtNjguMmw4NS0zN3Y3My4zek0xNTQgMTA0LjFsMTAyLTM4LjIgMTAyIDM4LjJ2LjZsLTEwMiA0MS40LTEwMi00MS40di0uNnptODQgMjkxLjFsLTg1IDQyLjV2LTc5LjFsODUtMzguOHY3NS40em0wLTExMmwtMTAyIDQxLjQtMTAyLTQxLjR2LS42bDEwMi0zOC4yIDEwMiAzOC4ydi42em0yNDAgMTEybC04NSA0Mi41di03OS4xbDg1LTM4Ljh2NzUuNHptMC0xMTJsLTEwMiA0MS40LTEwMi00MS40di0uNmwxMDItMzguMiAxMDIgMzguMnYuNnoiPjwvcGF0aD48L3N2Zz4K" height="20">](https://docs.rs/fixed-vec-deque)
[<img alt="build status" src="https://img.shields.io/github/actions/workflow/status/udoprog/fixed-vec-deque/ci.yml?branch=main&style=for-the-badge" height="20">](https://github.com/udoprog/fixed-vec-deque/actions?query=branch%3Amain)

A double-ended queue implemented with a fixed ring buffer.

This queue has `O(1)` amortized inserts and removals from both ends of the
container. It also has `O(1)` indexing like a vector. The contained elements
are not required to be copyable, and the queue will be sendable if the
contained type is sendable.

The size of the `FixedVecDeque` must be completely specified at construction
time, like this:

```rust
use fixed_vec_deque::FixedVecDeque;

let _ = FixedVecDeque::<[Foo; 4]>::new();

#[derive(Default)]
struct Foo;
```

Modifications can only happen _in-place_, this means that items stored in
the queue must always implement `Default`.

[`push_back`] and [`push_front`] don't take an argument, instead they return
a mutable reference so that the newly inserted element is mutated in-place:

```rust
use fixed_vec_deque::FixedVecDeque;

let mut buf = FixedVecDeque::<[Foo; 4]>::new();
buf.push_back().data = 42;

#[derive(Default)]
struct Foo {
    data: u32,
}
```

On a similar note, [`pop_front`] and [`pop_back`] returns references instead of moving the
elements.

A consequence of this is that this structure _never_ modifies the data it contains, even if it
has been _popped_.

<br>

## Missing APIs

[Some APIs are missing](https://github.com/udoprog/fixed-vec-deque/issues/2).
If you want to help out, leave a comment in the issue!

<br>

## When should I use `FixedVecDeque`?

Generally when the following holds:

* You have a maximum number of elements that you need to store for a short period of time.
* You only need to modify part of the element from the default when pushed.

A conventional collection require you to write a "complete" element every time it is added to
it.
With `FixedVecDeque` we can instead modify the existing elements in place, and keep track of
how many such logical "additions" we have done.
For example:

```rust
use fixed_vec_deque::FixedVecDeque;
use std::collections::VecDeque;

pub struct BigStruct {
    fields: [u64; 100],
}

impl Default for BigStruct {
    fn default() -> Self {
        BigStruct {
            fields: [0u64; 100],
        }
    }
}

let mut deq = FixedVecDeque::<[BigStruct; 0x10]>::new();

for i in 0..100 {
    deq.push_back().fields[i] = i as u64;

    let mut count = 0;

    for big in &deq {
        count += 1;
        assert_eq!(big.fields[i], i as u64);
    }

    assert_eq!(count, 1);
    deq.clear();
}

deq.clear();

// Note: modifications are still stored in the ring buffer and will be visible the next time we
// push to it unless we cleared it.
for i in 0..100 {
    assert_eq!(deq.push_back().fields[i], i as u64);
    deq.clear();
}
```

[`push_back`]: https://docs.rs/fixed-vec-deque/latest/fixed_vec_deque/struct.FixedVecDeque.html#method.push_back
[`push_front`]: https://docs.rs/fixed-vec-deque/latest/fixed_vec_deque/struct.FixedVecDeque.html#method.push_front
[`pop_back`]: https://docs.rs/fixed-vec-deque/latest/fixed_vec_deque/struct.FixedVecDeque.html#method.pop_back
[`pop_front`]: https://docs.rs/fixed-vec-deque/latest/fixed_vec_deque/struct.FixedVecDeque.html#method.pop_front
