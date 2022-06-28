// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{
    convert::TryFrom,
    fmt::{Display, Formatter},
    str::FromStr,
};

#[derive(Copy, Clone, Debug)]
pub enum TemplateId {
    Tip002 = 2,
    Tip003 = 3,
    Tip004 = 4,
    Tip721 = 721,
    EditableMetadata = 20,
}

impl FromStr for TemplateId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Tip002" => Ok(TemplateId::Tip002),
            "Tip003" => Ok(TemplateId::Tip003),
            "Tip004" => Ok(TemplateId::Tip004),
            "Tip721" => Ok(TemplateId::Tip721),
            "EditableMetadata" => Ok(TemplateId::EditableMetadata),
            _ => {
                println!("Unrecognised template");
                Err(format!("Unrecognised template ID '{}'", s))
            },
        }
    }
}

impl TryFrom<u32> for TemplateId {
    type Error = String;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            2 => Ok(TemplateId::Tip002),
            3 => Ok(TemplateId::Tip003),
            4 => Ok(TemplateId::Tip004),
            721 => Ok(TemplateId::Tip721),
            _ => Err(format!("Unknown value: {}", value)),
        }
    }
}

impl TryFrom<i32> for TemplateId {
    type Error = String;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        u32::try_from(value)
            .map_err(|_| {
                format!(
                    "Could not convert to TemplateId because it was not a valid u32:{}",
                    value
                )
            })?
            .try_into()
    }
}

impl Display for TemplateId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
