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

use std::iter;

use rand::{distributions::Alphanumeric, thread_rng, Rng};

/// Generate a random alphanumeric string of the given size using the default `ThreadRng`.
pub fn string(len: usize) -> String {
    let mut rng = thread_rng();
    iter::repeat(())
        .map(|_| rng.sample(Alphanumeric) as char)
        .take(len)
        .collect()
}

/// Generate a random alphanumeric string of the given size using the default `ThreadRng`.
pub fn prefixed_string(prefix: &str, len: usize) -> String {
    let mut rng = thread_rng();
    let rand_str = iter::repeat(())
        .map(|_| rng.sample(Alphanumeric) as char)
        .take(len)
        .collect::<String>();
    format!("{}{}", prefix, rand_str)
}

#[cfg(test)]
mod test {
    #[test]
    fn string() {
        let sample = super::string(8);
        assert_ne!(sample, super::string(8));
        assert_eq!(sample.len(), 8);
    }
}
