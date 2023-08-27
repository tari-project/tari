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

use std::{fmt, ops::Deref, slice, vec};

use serde::{
    de,
    de::{SeqAccess, Visitor},
    Deserialize,
    Deserializer,
    Serialize,
};

/// Supports deserialization from a sequence of strings or comma-delimited strings
#[derive(Debug, Default, Clone, Serialize, PartialEq, Eq)]
pub struct StringList(Vec<String>);

impl StringList {
    pub fn new() -> Self {
        Self(vec![])
    }

    pub fn with_capacity(size: usize) -> Self {
        Self(Vec::with_capacity(size))
    }

    pub fn into_vec(self) -> Vec<String> {
        self.0
    }

    pub fn as_slice(&self) -> &[String] {
        self.0.as_slice()
    }
}

impl Deref for StringList {
    type Target = [String];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<[String]> for StringList {
    fn as_ref(&self) -> &[String] {
        self.0.as_ref()
    }
}

impl From<Vec<String>> for StringList {
    fn from(v: Vec<String>) -> Self {
        Self(v)
    }
}

impl IntoIterator for StringList {
    type IntoIter = <Vec<String> as IntoIterator>::IntoIter;
    type Item = <Vec<String> as IntoIterator>::Item;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a StringList {
    type IntoIter = slice::Iter<'a, String>;
    type Item = <Self::IntoIter as Iterator>::Item;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'de> Deserialize<'de> for StringList {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: Deserializer<'de> {
        struct StringListVisitor;

        impl<'de> Visitor<'de> for StringListVisitor {
            type Value = StringList;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "a comma delimited string or multiple string elements")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where E: de::Error {
                Ok(StringList(
                    v.split(',')
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                        .map(ToString::to_string)
                        .collect(),
                ))
            }

            fn visit_newtype_struct<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where D: Deserializer<'de> {
                deserializer.deserialize_seq(StringListVisitor)
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where A: SeqAccess<'de> {
                let mut buf = seq.size_hint().map(Vec::with_capacity).unwrap_or_default();
                while let Some(v) = seq.next_element::<String>()? {
                    buf.push(v)
                }
                Ok(StringList(buf))
            }
        }

        if deserializer.is_human_readable() {
            deserializer.deserialize_seq(StringListVisitor)
        } else {
            deserializer.deserialize_newtype_struct("StringList", StringListVisitor)
        }
    }
}

#[cfg(test)]
mod tests {
    use config::Config;

    use super::*;

    #[derive(Deserialize)]
    struct Test {
        something: StringList,
    }

    #[test]
    fn with_capacity_test() {
        let new_str_lst = StringList::with_capacity(3);
        assert_eq!(new_str_lst.into_vec().capacity(), 3);
    }

    #[test]
    fn from_vec_string_list() {
        let vec_string = vec![String::from("Taiji is cool!")];
        let string_lst = StringList::from(vec_string);
        assert_eq!(string_lst.into_vec(), vec![String::from("Taiji is cool!")]);
    }

    #[test]
    fn as_ref_string_list() {
        let vec_string = vec![String::from("Taiji")];
        let vec_as_ref: &[String] = vec_string.as_ref();
        let string_lst = StringList::from(vec![String::from("Taiji")]);
        assert_eq!(string_lst.as_ref(), vec_as_ref);
    }

    #[test]
    fn into_iter_string_list() {
        let vec_string = vec![
            String::from("Taiji"),
            String::from("Project"),
            String::from("let's mine it!"),
        ];
        let string_lst = StringList::from(vec_string);
        let mut res_iter = string_lst.into_iter();

        assert_eq!(Some(String::from("Taiji")), res_iter.next());
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
