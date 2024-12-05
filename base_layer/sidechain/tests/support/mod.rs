// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use serde::de::DeserializeOwned;

pub fn load_fixture<T: DeserializeOwned>(name: &str) -> T {
    let path = format!("tests/fixtures/{name}");
    let file = std::fs::File::open(path).unwrap();
    serde_json::from_reader(file).unwrap()
}
