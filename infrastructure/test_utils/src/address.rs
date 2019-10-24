// Copyright 2019, The Tari Project
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

use std::{cmp, ops::Range, sync::Mutex};

const PORT_RANGE: Range<u16> = 40000..48000;
const LOCAL_ADDRESS: &'static str = "127.0.0.1";

lazy_static! {
    /// Shared counter of ports which have been used
    static ref PORT_COUNTER: Mutex<u16> = Mutex::new(PORT_RANGE.start);
}

/// Maintains a counter of ports within a range (40000..48000), returning them in
/// sequence. Port numbers will wrap back to 40000 once the upper bound is exceeded.
pub fn get_next_local_port() -> u16 {
    let mut lock = match PORT_COUNTER.lock() {
        Ok(guard) => guard,
        Err(_) => panic!("Poisoned PORT_COUNTER"),
    };
    let port = {
        *lock = cmp::max((*lock + 1) % PORT_RANGE.end, PORT_RANGE.start);
        *lock
    };
    port
}

/// Returns a local address with the next port in specified range.
pub fn get_next_local_address() -> String {
    format!("{}:{}", LOCAL_ADDRESS, get_next_local_port())
}

#[cfg(test)]
mod test {
    use crate::address::get_next_local_address;

    #[test]
    fn test_get_next_local_address() {
        let address1 = get_next_local_address();
        let address2 = get_next_local_address();
        assert_ne!(address1, address2);
    }
}
