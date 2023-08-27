// Copyright 2022 The Taiji Project
// SPDX-License-Identifier: BSD-3-Clause

/// Checks a request
pub trait Predicate<Request> {
    /// Check whether the given request should be forwarded.
    fn check(&mut self, request: &Request) -> bool;
}

impl<F, T> Predicate<T> for F
where F: Fn(&T) -> bool
{
    fn check(&mut self, request: &T) -> bool {
        self(request)
    }
}
