# fixed-vec-deque
[![Build Status](https://travis-ci.org/udoprog/fixed-vec-deque.svg?branch=master)](https://travis-ci.org/udoprog/fixed-vec-deque)
[![Documentation](https://docs.rs/fixed-vec-deque/badge.svg)](https://docs.rs/fixed-vec-deque)

**Note:** this crate is still in heavy development. Please be careful!

This crate provides a fixed-size VecDeque implementation (a.k.a. a fixed-size ring buffer) that
only provides referential access to what it is storing.

We try as much as possible to mimic the API of [VecDeque], but due to only dealing with
references, some differences are inevitable.

For information on how to use it, see the [Documentation].

[VecDeque]: https://doc.rust-lang.org/std/collections/struct.VecDeque.html
[documentation]: https://docs.rs/fixed-vec-deque

## Missing APIs

[Some APIs are missing](https://github.com/udoprog/fixed-vec-deque/issues/2).
If you want to help out, leave a comment in the issue!

## LICENSE

This project contains code derived from [VecDeque] (Rust stdlib) and [smallvec].

[VecDeque]: https://github.com/rust-lang/rust/blob/e8aef7cae14bc7a56859408c90253e9bcc07fcff/src/liballoc/collections/vec_deque.rs
[smallvec]: https://github.com/servo/rust-smallvec
