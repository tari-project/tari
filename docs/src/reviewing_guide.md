# Reviewing Guide

## Overview
The purpose of this document is to help reviewers look for common mistakes and vulnerabilities in the code base. This aims
to help upskill new developers and reviewers to the project. It is by no means a complete list of things to look for, but
it should help to get started.

This is a living document and should be updated as new issues are found and new things are learned.

## Reviewing Process

Many vulnerabilities start at the edges of the system, for example, an attacker may craft a data packet that causes a panic.
So a good place to start is to look at the entry points to the system and follow the packet through the flow of the system. 
This is generally much more effective than trying to look at a file in isolation. 

### Trusted vs Untrusted Data
Data received that is not generated
by the local system should be seen as untrusted and treated very carefully.

In addition to data packets received from the network, there are other sources of untrusted data. For example, a config 
setting may be copied from a post on the internet, or command line interface (CLI) commands may be copied from a forum.

Even the local data files that are generated in the data directory may be untrusted in some scenarios. For blockchains, it 
is not uncommon to download a copy of the blockchain from a third party as a data file. 

Another, less obvious, source of untrusted data are third party crates. Changes to dependencies should be monitored closely.

So what are some things to look out for when dealing with untrusted data?
1. Any parsing of untrusted data can panic, return an error or generate the wrong data. If the code parsing is inside the
Tari codebase, it should have matching tests and be fuzzed. If the code is in a third party crate, it should be reviewed very
carefully. Does the third party crate have fuzzing? Does it have tests? Is it actively maintained? 
    1. Be careful when using crates that wrap C libraries and other native code. A panic in native code can not be recovered 
and could be used to crash a node.
2. Be especially careful when reading the length for a buffer or Vec from untrusted data. If the length is too large, it could
cause an out of memory exception. If a length is received from an untrusted source, it should be checked against a maximum before allocating memory.
   1. E.g. `let length = read_u64(stream); let mut buf = vec![0u8; length];` should be replaced with `let mut buf = vec![0u8; min(length, MAX_LENGTH)];`


### Comparing to Diagrams
When reviewing a pull request, it is important to understand the context of the change. Arguably the easiest way to
achieve this is to review it in the context of the [diagrams](diagrams/README.md). 

Some questions to ask:

1. Firstly, is the diagram up to date with the code?
2. Does the PR require the diagram to be updated?

### Catching Common Mistakes
1. Using `unwrap` or `expect` in production code. These should only be used in tests and in code that is not reachable in production.
2. As stated before, watch out for parsing of untrusted data, especially when using a third party crate.
3. When parsing untrusted data it is extremely important that the code is fuzzed and that all branches of the code are covered by tests. Coverage reports are generated whenever code is commited to the development branch, but
PRs will need to be checked manually if they change this code.
4. What happens when functions are called with no data, too much data, repeated data, etc.?
5. Can some flow of data lead to a panic?
6. When using mutexes, readwrite locks or semaphores, can the lock be held for a long time? e.g. the lock is held while performing network IO with an untrusted peer.
7. When requesting data from another node, there are a few things to look out for:
   1. What happens if the node does not respond? Is there a timeout? The timeout should be set as low as possible, otherwise 
the node can hold up the processing on the requesting node.
   2. What happens if the node returns unexpected data?
8. In cases where we have received data from a node, whether we requested it or not, if it is bad, is the node banned?


### More specific things to look out for

#### Usize
`usize` is a platform dependent type. It is 32 bits on 32 bit platforms and 64 bits on 64 bit platforms. This can cause 
differences in hashing and serialisation. It should be avoided.

#### Atomics
It is fine to use `AtomicBool`, `AtomicUsize` and the other atomic types, but all `Orderings` must use `Ordering::SeqCst`.
See [nomicon](https://doc.rust-lang.org/nomicon/atomics.html) for more detail, but most of the ordering enum values have little visible effect on intel based architectures([x86](https://simple.wikipedia.org/wiki/X86) and [x86-64](https://en.wikipedia.org/wiki/X86-64)
and are only seen in arm based systems. It’s also unlikely that any performance is gained from using a different ordering, so rather be on the safe side and use `SeqCst` everywhere.

It is also pretty much impossible to test ordering on intel based systems, so it is best to avoid it.

#### Vec
When using `Vec::with_capacity`, is the size input provided by an untrusted party? If so, use a maximum bound on the capacity and the number of items returned or reject the message. The latter is almost always more appropriate. 

#### Unchecked arithmetic and overflows
Any arithmetic operation has a chance of overflowing or underflowing. It is best to use the checked versions, for example `checked_mul`, `checked_sub` etc. to avoid this.
Be careful of indexing into a slice or array with an untrusted value. This can cause a panic. Use `get` instead.

#### Shifts (<<, >>)
For some reason, `checked_shr` and `checked_shl` do not act like `checked_add` and `checked_sub` for overflows. They will only return `None` if the inputted shift is too large, but will 
shift even if the MSB is set. i.e. `0b1000_0000 << 1` will return `0b0000_0000`. This is often not what we expect. Use `leading_zeroes()` to check if the value would overflow before shifting.

#### Tokio
Tokio has many useful channels, and it can be difficult to know what to look out for, so here are some things to keep in mind:
1. When using `watch`:
   1. Watches can block if the reference returned from `borrow()` is held for a long time. Any call to `borrow()` should drop the reference as soon as possible.
2. When using `broadcast`:
   1. If one receiver is not receiving the values, it will return a `Lagged` error. This should be logged, but in most cases the code can continue as normal.
   2. When receiving events in a loop, Error::Closed should be used to break out of the loop, because the sender halves have dropped and no more events will be received
   2. Sending to a broadcast `Sender` before there are any receivers (or if they have all dropped) will error. If this is happening on startup, it should be 
handled gracefully and may succeed in future when a receiver is added.

#### Behind-the-scenes panics

Not all methods in the standard and other libraries that return values are guaranteed not to panic, for example, `pub const fn split_at(&self, mid: usize) -> (&[T], &[T])` will panic if `mid` > `self.len()`. Create custom wrappers that will return an error before the underlying function will panic, for example, `pub fn split_at_checked<T>(vec: &[T], n: usize) -> Result<(&[T], &[T]), Error>`.
