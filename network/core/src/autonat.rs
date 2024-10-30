// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

#[derive(Debug, Clone)]
pub enum AutonatStatus {
    ConfiguredPrivate,
    Checking,
    Private,
    Public,
}
