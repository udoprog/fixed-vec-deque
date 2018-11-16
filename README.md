# fixed-vec-deque

This crate provides a fixed-size VecDeque implementation (a.k.a. a fixed-size ring buffer) that
only provides referential access to what it is storing.

We try as much as possible to mimic the API of [VecDeque], but due to only dealing with
references, some differences are inevitable.

For information on how to use it, see the [Documentation].

[VecDeque]: https://doc.rust-lang.org/std/collections/struct.VecDeque.html
[documentation]: https://docs.rs/fixed-vec-deque

## LICENSE

This project contains code derived from [VecDeque] (Rust stdlib) and [smallvec].

[VecDeque]: https://github.com/rust-lang/rust/blob/e8aef7cae14bc7a56859408c90253e9bcc07fcff/src/liballoc/collections/vec_deque.rs
[smallvec]: https://github.com/servo/rust-smallvec
