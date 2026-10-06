// Copyright 2026 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

//! Serializes a sequence of byte arrays with each element encoded as in [`super::hex`].

use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub fn serialize<S: Serializer, T: AsRef<[u8]>>(v: &[T], s: S) -> Result<S::Ok, S::Error> {
    struct Item<'a>(&'a [u8]);

    impl Serialize for Item<'_> {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            super::hex::serialize(&self.0, s)
        }
    }

    s.collect_seq(v.iter().map(|item| Item(item.as_ref())))
}

pub fn deserialize<'de, D, T>(d: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: TryFrom<Vec<u8>>,
{
    struct Item<T>(T);

    impl<'de, T: TryFrom<Vec<u8>>> Deserialize<'de> for Item<T> {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            super::hex::deserialize(d).map(Item)
        }
    }

    let items = Vec::<Item<T>>::deserialize(d)?;
    Ok(items.into_iter().map(|item| item.0).collect())
}
