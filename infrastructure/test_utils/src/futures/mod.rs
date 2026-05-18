// Copyright 2019 The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

mod async_assert_eventually;

/// Creates a test counter future Context.
///
/// ## Usage
/// ```edition2018
/// # use futures::future::{self, FutureExt};
/// # use tari_test_utils::counter_context;
///
/// {
///     let mut my_fut = future::ready(());
///     counter_context!(cx); // cx variable in scope
///     assert!(my_fut.poll_unpin(&mut cx).is_ready());
/// }
///
/// {
///     let mut my_fut = future::ready(());
///     counter_context!(cx, counter); // cx and counter variables in scope
///     assert!(my_fut.poll_unpin(&mut cx).is_ready());
///     assert_eq!(counter.get(), 0); // `poll` didn't call the waker
/// }
/// ```
#[macro_export]
macro_rules! counter_context {
    ($n:ident, $c:ident) => {
        use futures_test::task::new_count_waker;
        let (waker, $c) = new_count_waker();
        let mut $n = futures::task::Context::from_waker(&waker);
    };
    ($n:ident) => {
        use futures_test::task::new_count_waker;
        let (waker, _) = new_count_waker();
        let mut $n = futures::task::Context::from_waker(&waker);
    };
}

/// Creates a test counter future Context.
///
/// ## Usage
/// ```edition2018
/// # use futures::future::{self, FutureExt};
/// # use tari_test_utils::panic_context;
///
/// let mut my_fut = future::ready(());
/// panic_context!(cx); // cx variable in scope
/// assert!(my_fut.poll_unpin(&mut cx).is_ready());
/// ```
#[macro_export]
macro_rules! panic_context {
    ($n:ident) => {
        use futures_test::task::panic_waker;
        let waker = panic_waker();
        let mut $n = futures::task::Context::from_waker(&waker);
    };
}

#[cfg(test)]
mod test {
    use std::task::Poll;

    use futures::{
        future::{self, FutureExt},
        task::Context,
    };

    #[test]
    fn counter_context() {
        {
            let mut my_fut = future::ready(());
            counter_context!(cx); // cx variable in scope
            assert!(my_fut.poll_unpin(&mut cx).is_ready());
        }

        {
            let mut my_fut = future::ready(());
            counter_context!(cx, counter); // cx and counter variables in scope
            assert!(my_fut.poll_unpin(&mut cx).is_ready());
            assert_eq!(counter.get(), 0); // `poll` didn't call the waker
        }
    }

    #[test]
    #[should_panic]
    fn panic_context() {
        let mut my_fut = future::poll_fn::<(), _>(|cx: &mut Context<'_>| {
            cx.waker().wake_by_ref();
            Poll::Pending
        });
        panic_context!(cx);

        let _ = my_fut.poll_unpin(&mut cx);
    }
}
