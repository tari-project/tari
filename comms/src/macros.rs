//  Copyright 2019 The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

/// Creates a setter function used with the builder pattern
macro_rules! setter {
 ($func:ident, $name: ident, Option<$type: ty>) => {
        pub fn $func(mut self, val: $type) -> Self {
            self.$name = Some(val);
            self
        }
    };
 ($func:ident, $name: ident, $type: ty) => {
        pub fn $func(mut self, val: $type) -> Self {
            self.$name = val;
            self
        }
    };
}

macro_rules! acquire_lock {
    ($e:expr, $m:ident) => {
        match $e.$m() {
            Ok(lock) => lock,
            Err(poisoned) => poisoned.into_inner(),
        }
    };
    ($e:expr) => {
        acquire_lock!($e, lock)
    };
}

macro_rules! acquire_write_lock {
    ($e:expr) => {
        acquire_lock!($e, write)
    };
}

macro_rules! acquire_read_lock {
    ($e:expr) => {
        acquire_lock!($e, read)
    };
}

/// Log an error if an `Err` is returned from the `$expr`. If the given expression is `Ok(v)`,
/// `Some(v)` is returned, otherwise `None` is returned (same as `Result::ok`).
/// Useful in cases where the error should be logged and ignored.
/// instead of writing `if let Err(err) = my_error_call() { error!(...) }`, you can write
/// `log_if_error(my_error_call())` ```edition2018
/// # use futures::channel::oneshot;
/// # use tari_comms::log_if_error;
/// let (tx, _) = oneshot::channel();
/// // Sending on oneshot will fail because the receiver is dropped. This error will be logged.
/// let opt = log_if_error!(target: "debugging", "Error sending reply: {}", tx.send("my reply"));
/// assert_eq!(opt, None);
/// ```
#[macro_export]
macro_rules! log_if_error {
    (target: $target:expr, $msg:expr, $expr:expr, no_fmt_msg=true$(,)*) => {{
        match $expr {
            Ok(v) => Some(v),
            Err(err) => {
                log::error!(target: $target, $msg);
                None
            }
        }
    }};
    (target: $target:expr, $msg:expr, $expr:expr) => {{
        match $expr {
            Ok(v) => Some(v),
            Err(err) => {
                log::error!(target: $target, $msg, err);
                None
            }
        }
    }};
    ($msg:expr, $expr:expr) => {{
        log_if_error!(target: "$crate", $msg, $expr)
    }};
}
