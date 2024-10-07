// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{
    fmt::{Display, Formatter},
    ops::Deref,
    slice,
    str::FromStr,
};

use serde::{
    de::{self, SeqAccess, Visitor},
    Deserialize,
    Deserializer,
    Serialize,
};

// Define a new type ConfigList<T>
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ConfigList<T>(Vec<T>);

impl<T> ConfigList<T> {
    /// Create a new ConfigList<T>
    pub fn new() -> Self {
        Self(vec![])
    }

    /// Create a new ConfigList<T> with a specified capacity
    pub fn with_capacity(size: usize) -> Self {
        ConfigList(Vec::with_capacity(size))
    }

    /// Consume ConfigList<T> and convert into a Vec<T>
    pub fn into_vec(self) -> Vec<T> {
        self.0
    }

    /// Convert ConfigList<T> into a Vec<T>
    pub fn to_vec(&self) -> Vec<T>
    where T: Clone {
        self.0.clone()
    }

    /// Get a reference to the inner Vec<T>
    pub fn as_slice(&self) -> &[T] {
        &self.0
    }

    /// Get the length of the inner Vec<T>
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Get the length of the inner Vec<T>
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Get an iterator over the inner Vec<T>
    pub fn iter(&self) -> slice::Iter<'_, T> {
        self.0.iter()
    }
}

impl<T> Display for ConfigList<T>
where T: Display
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            self.0
                .iter()
                .map(|item| item.to_string())
                .collect::<Vec<String>>()
                .join(", ")
        )
    }
}

impl<T> Default for ConfigList<T> {
    fn default() -> Self {
        Self(vec![])
    }
}

// Implement Deref for ConfigList<T>
impl<T> Deref for ConfigList<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

// Implement AsRef<[T]> for ConfigList<T>
impl<T> AsRef<[T]> for ConfigList<T> {
    fn as_ref(&self) -> &[T] {
        self.as_slice()
    }
}

// Implement From<Vec<T>> for ConfigList<T>
impl<T> From<Vec<T>> for ConfigList<T> {
    fn from(v: Vec<T>) -> Self {
        ConfigList(v)
    }
}

// Implement IntoIterator for ConfigList<T>
impl<T> IntoIterator for ConfigList<T> {
    type IntoIter = <Vec<T> as IntoIterator>::IntoIter;
    type Item = <Vec<T> as IntoIterator>::Item;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

// Implement IntoIterator for &ConfigList<T>
impl<'a, T> IntoIterator for &'a ConfigList<T> {
    type IntoIter = slice::Iter<'a, T>;
    type Item = <Self::IntoIter as Iterator>::Item;

    fn into_iter(self) -> Self::IntoIter {
        self.as_slice().iter()
    }
}

// Implement Deserialize<'de> for ConfigList<T>
impl<'de, T> Deserialize<'de> for ConfigList<T>
where
    T: FromStr + Deserialize<'de>,
    <T as FromStr>::Err: Display,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: Deserializer<'de> {
        struct ConfigListVisitor<T>(std::marker::PhantomData<T>);

        impl<'de, T> Visitor<'de> for ConfigListVisitor<T>
        where
            T: FromStr + Deserialize<'de>,
            <T as FromStr>::Err: Display,
        {
            type Value = ConfigList<T>;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "a comma delimited string or multiple string elements")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where E: de::Error {
                if v.trim().is_empty() {
                    return Ok(ConfigList::new());
                }
                let strings = v
                    .split(',')
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>();
                let parsed: Result<Vec<_>, _> = strings
                    .into_iter()
                    .map(|item| T::from_str(item).map_err(E::custom))
                    .collect();
                Ok(ConfigList(parsed.map_err(E::custom)?))
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where A: SeqAccess<'de> {
                let mut buf = seq.size_hint().map(Vec::with_capacity).unwrap_or_default();
                while let Some(v) = seq.next_element::<T>()? {
                    buf.push(v)
                }
                Ok(ConfigList::from(buf))
            }
        }

        if deserializer.is_human_readable() {
            deserializer.deserialize_seq(ConfigListVisitor(std::marker::PhantomData))
        } else {
            deserializer.deserialize_newtype_struct("ConfigList", ConfigListVisitor(std::marker::PhantomData))
        }
    }
}

#[cfg(test)]
mod test_config_list_general {
    use config::Config;
    use serde::Deserialize;

    use crate::configuration::ConfigList;

    type TestList = ConfigList<String>;

    #[derive(Debug, Deserialize, PartialEq)]
    struct Test {
        something: TestList,
    }

    #[test]
    fn with_capacity_test() {
        let new_str_lst = TestList::with_capacity(3);
        assert_eq!(new_str_lst.into_vec().capacity(), 3);
    }

    #[test]
    fn from_vec_string_list() {
        let vec_string = vec![String::from("Tari is cool!")];
        let string_lst = TestList::from(vec_string);
        assert_eq!(string_lst.into_vec(), vec![String::from("Tari is cool!")]);
    }

    #[test]
    fn as_ref_string_list() {
        let vec_string = vec![String::from("Tari")];
        let vec_as_ref: &[String] = vec_string.as_ref();
        let string_lst = TestList::from(vec![String::from("Tari")]);
        assert_eq!(string_lst.as_ref(), vec_as_ref);
    }

    #[test]
    fn into_iter_string_list() {
        let vec_string = vec![
            String::from("Tari"),
            String::from("Project"),
            String::from("let's mine it!"),
        ];
        let string_lst = TestList::from(vec_string);
        let mut res_iter = string_lst.into_iter();

        assert_eq!(Some(String::from("Tari")), res_iter.next());
        assert_eq!(Some(String::from("Project")), res_iter.next());
        assert_eq!(Some(String::from("let's mine it!")), res_iter.next());
        assert_eq!(None, res_iter.next());
    }

    #[test]
    fn it_deserializes_from_toml() {
        let config_str = r#"something = ["a","b","c"]"#;
        let test = toml::from_str::<Test>(config_str).unwrap();
        assert_eq!(test.something.0, vec!["a", "b", "c"]);
    }

    #[test]
    fn it_deserializes_from_config_comma_delimited() {
        let config = Config::builder()
            .set_override("something", "a, b, c,")
            .unwrap()
            .build()
            .unwrap();
        let test = config.try_deserialize::<Test>().unwrap();
        assert_eq!(test.something.0, vec!["a", "b", "c"]);
    }
}

#[cfg(test)]
mod test_config_list_for_toml {
    use std::str::FromStr;

    use serde::Deserialize;

    use crate::configuration::{ConfigList, Multiaddr};

    #[derive(Debug, Deserialize, PartialEq)]
    struct Test {
        #[serde(default)]
        u32_list: ConfigList<u32>,
        #[serde(default)]
        string_list: ConfigList<String>,
        #[serde(default)]
        multiaddr_list: ConfigList<Multiaddr>,
    }

    #[test]
    fn it_deserializes_from_toml() {
        // No empty fields, no omitted fields
        let config_str = r#"
            # u32
            u32_list = [1, 2, 3, 4, 5]
            # String
            string_list = ["1", "2", "3", "4", "5"]
            # Multiaddr
            multiaddr_list = ["/ip4/127.0.150.1/tcp/18500", "/ip4/127.0.0.1/udt/sctp/5678", "/ip4/127.0.0.0/tcp/18189"]
         "#;
        let config = toml::from_str::<Test>(config_str).unwrap();
        let item_vec_u32 = config.u32_list.into_vec();
        assert_eq!(item_vec_u32, vec![1, 2, 3, 4, 5]);
        let item_vec_string = config.string_list.into_vec();
        assert_eq!(item_vec_string, vec!["1", "2", "3", "4", "5"]);
        let item_vec_multiaddr = config.multiaddr_list.into_vec();
        assert_eq!(item_vec_multiaddr, vec![
            Multiaddr::from_str("/ip4/127.0.150.1/tcp/18500").unwrap(),
            Multiaddr::from_str("/ip4/127.0.0.1/udt/sctp/5678").unwrap(),
            Multiaddr::from_str("/ip4/127.0.0.0/tcp/18189").unwrap(),
        ]);

        // Empty fields, omitted fields, filled fields
        let config_str = r#"
            # u32
            u32_list = [1, 2, 3, 4, 5]
            # String
            string_list = []
            # Multiaddr
            #multiaddr_list = []
         "#;
        let config = toml::from_str::<Test>(config_str).unwrap();
        let item_vec_u32 = config.u32_list.into_vec();
        assert_eq!(item_vec_u32, vec![1, 2, 3, 4, 5]);
        let item_vec_string = config.string_list.into_vec();
        assert_eq!(item_vec_string, Vec::<String>::new());
        let item_vec_multiaddr = config.multiaddr_list.into_vec();
        assert_eq!(item_vec_multiaddr, Vec::<Multiaddr>::new());
    }
}
