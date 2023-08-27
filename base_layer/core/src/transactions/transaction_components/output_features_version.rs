// Copyright 2022 The Taiji Project
// SPDX-License-Identifier: BSD-3-Clause

use std::convert::TryFrom;

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use strum_macros::Display;

#[derive(
    Debug,
    Clone,
    Copy,
    Hash,
    PartialEq,
    Deserialize,
    Serialize,
    Eq,
    PartialOrd,
    Display,
    BorshSerialize,
    BorshDeserialize,
)]
#[repr(u8)]
pub enum OutputFeaturesVersion {
    V0 = 0,
    V1 = 1,
}

impl OutputFeaturesVersion {
    pub fn get_current_version() -> Self {
        Self::V0
    }

    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

impl TryFrom<u8> for OutputFeaturesVersion {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(OutputFeaturesVersion::V0),
            1 => Ok(OutputFeaturesVersion::V1),
            _ => Err("Unknown or unsupported OutputFeaturesVersion".into()),
        }
    }
}
