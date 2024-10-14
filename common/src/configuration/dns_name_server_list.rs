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

use std::{fmt, str::FromStr};

use serde::{de, de::Visitor, Deserializer};

use crate::{configuration::ConfigList, DnsNameServer};

pub type DnsNameServerList = ConfigList<DnsNameServer>;

impl FromStr for DnsNameServerList {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let dns_list = s
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(DnsNameServer::from_str)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(DnsNameServerList::from(dns_list))
    }
}

pub fn deserialize_dns_name_server_list<'de, D>(deserializer: D) -> Result<DnsNameServerList, D::Error>
where D: Deserializer<'de> {
    struct DnsNameServerVisitor;

    impl<'de> Visitor<'de> for DnsNameServerVisitor {
        type Value = DnsNameServerList;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a valid DNS name server list string or sequence")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where E: de::Error {
            DnsNameServerList::from_str(value).map_err(de::Error::custom)
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where A: de::SeqAccess<'de> {
            let mut dns_list = Vec::new();
            while let Some(value) = seq.next_element::<String>()? {
                let dns_name_server = DnsNameServer::from_str(&value).map_err(de::Error::custom)?;
                dns_list.push(dns_name_server);
            }
            Ok(DnsNameServerList::from(dns_list))
        }
    }

    deserializer.deserialize_any(DnsNameServerVisitor)
}

#[cfg(test)]
mod tests {
    use std::{str::FromStr, vec};

    use config::Config;
    use serde::Deserialize;

    use crate::{
        configuration::{dns_name_server_list::deserialize_dns_name_server_list, DnsNameServerList},
        DnsNameServer,
    };

    #[derive(Deserialize, Debug)]
    struct Test {
        #[serde(deserialize_with = "deserialize_dns_name_server_list")]
        something: DnsNameServerList,
    }

    #[test]
    fn with_capacity_test() {
        let new_str_lst = DnsNameServerList::with_capacity(3);
        assert_eq!(new_str_lst.into_vec().capacity(), 3);
    }

    #[test]
    fn default_test() {
        let dns_list = DnsNameServerList::default();
        assert_eq!(dns_list.into_vec(), vec![]);
    }

    #[test]
    fn from_vec_string_list() {
        let vec_dns_list = vec![
            DnsNameServer::from_str("127.0.0.1:8080/my_dns").unwrap(),
            DnsNameServer::from_str("system").unwrap(),
            DnsNameServer::from_str("1.1.1.1:853/cloudflare-dns.com").unwrap(),
        ];
        let dns_list = DnsNameServerList::from(vec_dns_list);
        assert_eq!(dns_list.into_vec(), vec![
            DnsNameServer::from_str("127.0.0.1:8080/my_dns").unwrap(),
            DnsNameServer::from_str("system").unwrap(),
            DnsNameServer::from_str("1.1.1.1:853/cloudflare-dns.com").unwrap(),
        ]);
    }

    #[test]
    fn as_ref_dns_list() {
        let vec_dns_list = vec![DnsNameServer::from_str("127.0.0.1:8080/my_dns").unwrap()];
        let vec_as_ref: &[DnsNameServer] = vec_dns_list.as_ref();
        let dns_list = DnsNameServerList::from(vec![DnsNameServer::from_str("127.0.0.1:8080/my_dns").unwrap()]);
        assert_eq!(dns_list.as_ref(), vec_as_ref);
    }

    #[test]
    fn into_iter_dns_list() {
        let vec_dns_list = vec![
            DnsNameServer::from_str("127.0.0.1:8080/my_dns").unwrap(),
            DnsNameServer::from_str("system").unwrap(),
            DnsNameServer::from_str("1.1.1.1:853/cloudflare-dns.com").unwrap(),
        ];
        let dns_list = DnsNameServerList::from(vec_dns_list);
        let mut res_iter = dns_list.into_iter();

        assert_eq!(
            Some(DnsNameServer::from_str("127.0.0.1:8080/my_dns").unwrap()),
            res_iter.next()
        );
        assert_eq!(Some(DnsNameServer::from_str("system").unwrap()), res_iter.next());
        assert_eq!(
            Some(DnsNameServer::from_str("1.1.1.1:853/cloudflare-dns.com").unwrap()),
            res_iter.next()
        );
        assert_eq!(None, res_iter.next());
    }

    #[test]
    fn it_deserializes_from_toml() {
        let config_str = r#"" 127.0.0.1:8080/my_dns", 'SYSTEM', 1.1.1.1:853/cloudflare-dns.COM ""#;
        DnsNameServerList::from_str(config_str).unwrap();

        let config_str = r#"something = ["127.0.0.1:8080/my_dns", "system", "1.1.1.1:853/cloudflare-dns.com"]"#;
        let test = toml::from_str::<Test>(config_str).unwrap();
        assert_eq!(test.something.into_vec(), vec![
            DnsNameServer::from_str("127.0.0.1:8080/my_dns").unwrap(),
            DnsNameServer::from_str("system").unwrap(),
            DnsNameServer::from_str("1.1.1.1:853/cloudflare-dns.com").unwrap(),
        ]);
    }

    #[test]
    fn it_returns_error() {
        let config_str = r#"something = ["Not dns","system","1.1.1.1:853/cloudflare-dns.com"]"#;
        assert!(toml::from_str::<Test>(config_str).is_err());
    }

    #[test]
    fn it_deserializes_from_config_comma_delimited() {
        let config = Config::builder()
            .set_override(
                "something",
                "127.0.0.1:8080/my_dns, system, 1.1.1.1:853/cloudflare-dns.com,",
            )
            .unwrap()
            .build()
            .unwrap();
        let test = config.try_deserialize::<Test>().unwrap();
        assert_eq!(test.something.into_vec(), vec![
            DnsNameServer::from_str("127.0.0.1:8080/my_dns").unwrap(),
            DnsNameServer::from_str("system").unwrap(),
            DnsNameServer::from_str("1.1.1.1:853/cloudflare-dns.com").unwrap(),
        ]);
    }
}
