//  Copyright 2022. The Taiji Project
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

use std::{fmt, ops::Deref, slice, str::FromStr, vec};

use multiaddr::Multiaddr;
use serde::{
    de,
    de::{SeqAccess, Visitor},
    Deserialize,
    Deserializer,
    Serialize,
};

/// Supports deserialization from a sequence of strings or comma-delimited strings
#[derive(Debug, Default, Clone, Serialize, PartialEq, Eq)]
pub struct MultiaddrList(Vec<Multiaddr>);

impl MultiaddrList {
    pub fn new() -> Self {
        Self(vec![])
    }

    pub fn with_capacity(size: usize) -> Self {
        Self(Vec::with_capacity(size))
    }

    pub fn into_vec(self) -> Vec<Multiaddr> {
        self.0
    }

    pub fn as_slice(&self) -> &[Multiaddr] {
        self.0.as_slice()
    }
}

impl Deref for MultiaddrList {
    type Target = [Multiaddr];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<[Multiaddr]> for MultiaddrList {
    fn as_ref(&self) -> &[Multiaddr] {
        self.0.as_ref()
    }
}

impl From<Vec<Multiaddr>> for MultiaddrList {
    fn from(v: Vec<Multiaddr>) -> Self {
        Self(v)
    }
}

impl IntoIterator for MultiaddrList {
    type IntoIter = <Vec<Multiaddr> as IntoIterator>::IntoIter;
    type Item = <Vec<Multiaddr> as IntoIterator>::Item;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a MultiaddrList {
    type IntoIter = slice::Iter<'a, Multiaddr>;
    type Item = <Self::IntoIter as Iterator>::Item;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'de> Deserialize<'de> for MultiaddrList {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: Deserializer<'de> {
        struct MultiaddrListVisitor;

        impl<'de> Visitor<'de> for MultiaddrListVisitor {
            type Value = MultiaddrList;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "a comma delimited multiaddr or multiple multiaddr elements")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where E: de::Error {
                Ok(MultiaddrList(
                    v.split(',')
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                        .map(Multiaddr::from_str)
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|e| E::invalid_value(de::Unexpected::Str(e.to_string().as_str()), &self))?,
                ))
            }

            fn visit_newtype_struct<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where D: Deserializer<'de> {
                deserializer.deserialize_seq(MultiaddrListVisitor)
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where A: SeqAccess<'de> {
                let mut buf = seq.size_hint().map(Vec::with_capacity).unwrap_or_default();
                while let Some(v) = seq.next_element::<Multiaddr>()? {
                    buf.push(v)
                }
                Ok(MultiaddrList(buf))
            }
        }

        if deserializer.is_human_readable() {
            deserializer.deserialize_seq(MultiaddrListVisitor)
        } else {
            deserializer.deserialize_newtype_struct("MultiaddrList", MultiaddrListVisitor)
        }
    }
}

#[cfg(test)]
mod tests {
    use config::Config;

    use super::*;

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
        assert_eq!(test.something.0, vec![
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
        assert_eq!(test.something.0, vec![
            Multiaddr::from_str("/ip4/127.0.0.1/tcp/1234").unwrap(),
            Multiaddr::from_str("/ip4/192.168.0.1/tcp/1234").unwrap(),
            Multiaddr::from_str("/ip4/10.0.0.1/tcp/1234").unwrap()
        ]);
    }
}
