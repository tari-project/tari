// Copyright 2020, The Tari Project
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

use std::str::FromStr;

pub fn parse_cidrs<'a, I: IntoIterator<Item = T>, T: AsRef<str>>(cidr_strs: I) -> Result<Vec<cidr::AnyIpCidr>, String> {
    let (success, failed) = cidr_strs
        .into_iter()
        .map(|s| ::cidr::AnyIpCidr::from_str(s.as_ref()))
        .partition::<Vec<_>, _>(Result::is_ok);

    if failed.len() > 0 {
        return Err(format!("Invalid CIDR strings: {:?}", failed));
    }

    Ok(success.into_iter().map(Result::unwrap).collect())
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn parse() {
        let cidrs = ["127.0.0.1/32", "2620:0:0:0::0/16"];
        let cidrs = parse_cidrs(&cidrs).unwrap();
        assert_eq!(cidrs[0].network_length(), Some(32));
        assert_eq!(cidrs[1].network_length(), Some(16));
        let cidrs = ["127.0.0.1/32", "127.0-0.1/32", "127.0.0.1?32", "2620:0:2d0:200::7/32"];
        parse_cidrs(&cidrs).unwrap_err();
    }
}
