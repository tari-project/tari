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

use std::{net::IpAddr, str::FromStr};

pub fn parse_cidrs<I: IntoIterator<Item = T>, T: AsRef<str>>(cidr_strs: I) -> Result<Vec<cidr::AnyIpCidr>, String> {
    let (success, failed) = cidr_strs
        .into_iter()
        .map(|s| ::cidr::AnyIpCidr::from_str(s.as_ref()))
        .partition::<Vec<_>, _>(Result::is_ok);

    if !failed.is_empty() {
        return Err(format!("Invalid CIDR strings: {failed:?}"));
    }

    Ok(success
        .into_iter()
        .map(Result::unwrap)
        .map(normalise_ipv4_mapped_cidr)
        .collect())
}

/// Returns the IPv4 form of a CIDR inside the IPv4-mapped IPv6 range (`::ffff:0:0/96`), e.g. `::ffff:10.0.0.0/104`
/// becomes `10.0.0.0/8`. Source addresses are compared in canonical form (IPv4-mapped addresses as IPv4), so an
/// IPv4-mapped CIDR would otherwise never match. Any other CIDR is returned unchanged.
pub fn normalise_ipv4_mapped_cidr(cidr: ::cidr::AnyIpCidr) -> ::cidr::AnyIpCidr {
    const IPV4_MAPPED_PREFIX_LENGTH: u8 = 96;
    let (Some(IpAddr::V6(first_address)), Some(length)) = (cidr.first_address(), cidr.network_length()) else {
        return cidr;
    };
    let Some(ipv4) = first_address.to_ipv4_mapped() else {
        return cidr;
    };
    if length < IPV4_MAPPED_PREFIX_LENGTH {
        return cidr;
    }
    match ::cidr::AnyIpCidr::new(IpAddr::V4(ipv4), length.saturating_sub(IPV4_MAPPED_PREFIX_LENGTH)) {
        Ok(ipv4_cidr) => ipv4_cidr,
        Err(_) => cidr,
    }
}

/// Applies [normalise_ipv4_mapped_cidr] to every CIDR in `cidrs`.
pub fn normalise_ipv4_mapped_cidrs(cidrs: &mut [::cidr::AnyIpCidr]) {
    for cidr in cidrs.iter_mut() {
        *cidr = normalise_ipv4_mapped_cidr(cidr.clone());
    }
}

#[cfg(test)]
mod test {
    #![allow(clippy::indexing_slicing)]
    use super::*;

    #[test]
    fn parse() {
        let cidrs = ["127.0.0.1/32", "2620:0:0:0::0/16"];
        let cidrs = parse_cidrs(cidrs).unwrap();
        assert_eq!(cidrs[0].network_length(), Some(32));
        assert_eq!(cidrs[1].network_length(), Some(16));
        let cidrs = ["127.0.0.1/32", "127.0-0.1/32", "127.0.0.1?32", "2620:0:2d0:200::7/32"];
        parse_cidrs(cidrs).unwrap_err();
    }

    #[test]
    fn ipv4_mapped_cidrs_are_normalised_to_ipv4() {
        let cidrs = parse_cidrs([
            "::ffff:10.0.0.0/104",
            "::ffff:127.0.0.1/128",
            "::ffff:0.0.0.0/96",
            "2001:db8::/32",
            "10.0.0.0/8",
        ])
        .unwrap();
        let expected = parse_cidrs(["10.0.0.0/8", "127.0.0.1/32", "0.0.0.0/0", "2001:db8::/32", "10.0.0.0/8"]).unwrap();
        assert_eq!(cidrs, expected);
        let ip: IpAddr = "10.1.2.3".parse().unwrap();
        assert!(cidrs[0].contains(&ip));

        // The config path (not parsed from strings) is normalised too
        let mut cidrs = vec![::cidr::AnyIpCidr::from_str("::ffff:192.168.0.0/112").unwrap()];
        normalise_ipv4_mapped_cidrs(&mut cidrs);
        assert_eq!(cidrs, vec![::cidr::AnyIpCidr::from_str("192.168.0.0/16").unwrap()]);
    }
}
