//  Copyright 2021, The Taiji Project
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

use std::marker::PhantomData;

use crate::multiaddr::{Multiaddr, Protocol};

pub trait Predicate<A: ?Sized> {
    fn check(&self, arg: &A) -> bool;
}

impl<T, A> Predicate<A> for T
where
    T: Fn(&A) -> bool,
    A: ?Sized,
{
    fn check(&self, arg: &A) -> bool {
        (self)(arg)
    }
}

#[derive(Debug, Default)]
pub struct FalsePredicate<'a, A>(PhantomData<&'a A>);

impl<'a, A> FalsePredicate<'a, A> {
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

impl<A> Predicate<A> for FalsePredicate<'_, A> {
    fn check(&self, _: &A) -> bool {
        false
    }
}

pub fn is_onion_address(addr: &Multiaddr) -> bool {
    let protocol = addr.iter().next();
    matches!(protocol, Some(Protocol::Onion(_, _)) | Some(Protocol::Onion3(_)))
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn is_onion_address_test() {
        let expect_true = [
            "/onion/aaimaq4ygg2iegci:1234",
            "/onion3/vww6ybal4bd7szmgncyruucpgfkqahzddi37ktceo3ah7ngmcopnpyyd:1234",
        ];

        let expect_false = ["/dns4/mikes-node-nook.com:80", "/ip4/1.2.3.4/tcp/1234"];

        expect_true.iter().for_each(|addr| {
            let addr = addr.parse().unwrap();
            assert!(is_onion_address(&addr));
        });

        expect_false.iter().for_each(|addr| {
            let addr = addr.parse().unwrap();
            assert!(!is_onion_address(&addr));
        });
    }
}
