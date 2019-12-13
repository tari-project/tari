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

/// Creates a setter function used with the builder pattern.
/// The value is moved into the function and returned out.
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

/// Creates a setter function used with the builder pattern
/// A mutable reference is taken and returned
#[cfg(feature = "next")]
macro_rules! setter_mut {
    ($func:ident, $name: ident, Option<$type: ty>) => {
        #[allow(dead_code)]
        pub fn $func(&mut self, val: $type) -> &mut Self {
            self.$name = Some(val);
            self
        }
    };
    ($func:ident, $name: ident, $type: ty) => {
        #[allow(dead_code)]
        pub fn $func(&mut self, val: $type) -> &mut Self {
            self.$name = val;
            self
        }
    };
}

macro_rules! recover_lock {
    ($e:expr) => {
        match $e {
            Ok(lock) => lock,
            Err(poisoned) => {
                log::warn!(target: "comms", "Lock has been POISONED and will be silently recovered");
                poisoned.into_inner()
            },
        }
    };
}

macro_rules! acquire_lock {
    ($e:expr, $m:ident) => {
        recover_lock!($e.$m())
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
/// `log_if_error!(my_error_call())`
///
/// ```edition2018
/// # use tari_common::log_if_error;
/// let opt = log_if_error!(level: debug, target: "docs", "Error sending reply: {}", Result::<(), _>::Err("this will be logged"));
/// assert_eq!(opt, None);
/// ```
#[macro_export]
macro_rules! log_if_error {
    (level:$level:ident, target:$target:expr, $expr:expr, $msg:expr, $($args:tt),* $(,)*) => {{
        match $expr {
            Ok(v) => Some(v),
            Err(err) => {
                log::$level!(target: $target, $msg, $($args,)* error = err);
                None
            }
        }
    }};
    (target:$target:expr, $expr:expr, $msg:expr, $($args:tt),* $(,)*) => {{
        log_if_error!(level:error, target:$target, $expr, $msg, $($args),*)
    }};
    (level:$level:ident, $expr:expr, $msg:expr, $($args:tt),* $(,)*) => {{
        log_if_error!(level:$level, target:"$crate", $expr, $msg, $($args),*)
    }};
    ($expr:expr, $msg:expr, $($args:tt)* $(,)*) => {{
        log_if_error!(level:error, target:"$crate", $expr, $msg, $($args),*)
    }};
}

#[macro_export]
macro_rules! log_if_error_fmt {
    (level: $level:ident, target: $target:expr, $expr:expr, $($args:tt)+) => {{
        match $expr {
            Ok(v) => Some(v),
            Err(_) => {
                log::$level!(target: $target, $($args)+);
                None
            }
        }
    }};
    (level:$level:ident, $expr:expr, $($args:tt)+) => {{
        log_if_error_fmt!(level:$level, target: "$crate", $expr, $($args)+)
    }};
    (target: $target:expr, $expr:expr , $($args:tt)+) => {{
        log_if_error_fmt!(level:error, target: $target, $expr, $($args)+)
    }};
    ($msg:expr, $expr:expr, $($args:tt)+) => {{
        log_if_error_fmt!(level:error, target: "$crate", $expr, $($args)+)
    }};
}

/// Adds #[cfg(feature = "next")] to mod and use
macro_rules! cfg_next {
    ($($item:item)+) => {
        $(
            #[cfg(feature = "next")]
            $item
        )+
    }
}
