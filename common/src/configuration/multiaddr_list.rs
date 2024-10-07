//  Copyright 2022. The Tari Project
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

use libp2p::multiaddr::Multiaddr;

use crate::configuration::ConfigList;

/// Supports deserialization from a sequence of strings or comma-delimited strings
pub type MultiaddrList = ConfigList<Multiaddr>;

#[cfg(test)]
mod tests {
    use std::{str::FromStr, vec};

    use config::Config;
    use multiaddr::Multiaddr;
    use serde::Deserialize;

    use crate::configuration::MultiaddrList;

    #[derive(Deserialize)]
    struct Test {
        something: MultiaddrList,
    }

    #[test]
    fn with_capacity_test() {
        let new_str_lst = MultiaddrList::with_capacity(3);
        assert_eq!(new_str_lst.into_vec().capacity(), 3);
    }

    #[test]
    fn from_vec_string_list() {
        let vec_multiaddr = vec![Multiaddr::from_str("/ip4/127.0.0.1/tcp/1234").unwrap()];
        let multiaddr_lst = MultiaddrList::from(vec_multiaddr);
        assert_eq!(multiaddr_lst.into_vec(), vec![Multiaddr::from_str(
            "/ip4/127.0.0.1/tcp/1234"
        )
        .unwrap()]);
    }

    #[test]
    fn as_ref_multiaddr_list() {
        let vec_multiaddr = vec![Multiaddr::from_str("/ip4/127.0.0.1/tcp/1234").unwrap()];
        let vec_as_ref: &[Multiaddr] = vec_multiaddr.as_ref();
        let multiaddr_lst = MultiaddrList::from(vec![Multiaddr::from_str("/ip4/127.0.0.1/tcp/1234").unwrap()]);
        assert_eq!(multiaddr_lst.as_ref(), vec_as_ref);
    }

    #[test]
    fn into_iter_multiaddr_list() {
        let vec_multiaddr = vec![
            Multiaddr::from_str("/ip4/127.0.0.1/tcp/1234").unwrap(),
            Multiaddr::from_str("/ip4/192.168.0.1/tcp/1234").unwrap(),
            Multiaddr::from_str("/ip4/10.0.0.1/tcp/1234").unwrap(),
        ];
        let multiaddr_lst = MultiaddrList::from(vec_multiaddr);
        let mut res_iter = multiaddr_lst.into_iter();

        assert_eq!(
            Some(Multiaddr::from_str("/ip4/127.0.0.1/tcp/1234").unwrap()),
            res_iter.next()
        );
        assert_eq!(
            Some(Multiaddr::from_str("/ip4/192.168.0.1/tcp/1234").unwrap()),
            res_iter.next()
        );
        assert_eq!(
            Some(Multiaddr::from_str("/ip4/10.0.0.1/tcp/1234").unwrap()),
            res_iter.next()
        );
        assert_eq!(None, res_iter.next());
    }

    #[test]
    fn it_deserializes_from_toml() {
        let config_str =
            r#"something = ["/ip4/127.0.0.1/tcp/1234","/ip4/192.168.0.1/tcp/1234","/ip4/10.0.0.1/tcp/1234"]"#;
        let test = toml::from_str::<Test>(config_str).unwrap();
        assert_eq!(test.something.into_vec(), vec![
            Multiaddr::from_str("/ip4/127.0.0.1/tcp/1234").unwrap(),
            Multiaddr::from_str("/ip4/192.168.0.1/tcp/1234").unwrap(),
            Multiaddr::from_str("/ip4/10.0.0.1/tcp/1234").unwrap()
        ]);
    }

    #[test]
    fn it_returns_error() {
        let config_str = r#"something = ["Not multiaddr","/ip4/192.168.0.1/tcp/1234","/ip4/10.0.0.1/tcp/1234"]"#;
        assert!(toml::from_str::<Test>(config_str).is_err());
    }

    #[test]
    fn it_deserializes_from_config_comma_delimited() {
        let config = Config::builder()
            .set_override(
                "something",
                "/ip4/127.0.0.1/tcp/1234, /ip4/192.168.0.1/tcp/1234, /ip4/10.0.0.1/tcp/1234,",
            )
            .unwrap()
            .build()
            .unwrap();
        let test = config.try_deserialize::<Test>().unwrap();
        assert_eq!(test.something.into_vec(), vec![
            Multiaddr::from_str("/ip4/127.0.0.1/tcp/1234").unwrap(),
            Multiaddr::from_str("/ip4/192.168.0.1/tcp/1234").unwrap(),
            Multiaddr::from_str("/ip4/10.0.0.1/tcp/1234").unwrap()
        ]);
    }
}
