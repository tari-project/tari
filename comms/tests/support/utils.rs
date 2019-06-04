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

use std::{
    cmp,
    net::{TcpListener, ToSocketAddrs},
    sync::Mutex,
};
use tari_comms::connection::net_address::NetAddress;

lazy_static! {
    /// Shared counter of ports which have been used
    static ref PORT_COUNTER: Mutex<u16> = Mutex::new(20000);
}

/// Search for an available port on the given host. After 100 searches give up.
/// This function is thread-safe.
pub fn find_available_tcp_net_address(host: &str) -> Option<NetAddress> {
    let mut lock = match PORT_COUNTER.lock() {
        Ok(guard) => guard,
        Err(_) => panic!("Poisoned PORT_COUNTER"),
    };
    // Try 100
    for _i in 0..100 {
        let port = {
            *lock = cmp::max((*lock + 1) % std::u16::MAX, 20000u16);
            *lock
        };
        let addr = format!("{}:{}", host, port);
        if is_port_open(&addr) {
            return addr.parse().ok();
        }
    }
    None
}

pub fn is_port_open<A: ToSocketAddrs>(addr: A) -> bool {
    TcpListener::bind(addr).is_ok()
}
