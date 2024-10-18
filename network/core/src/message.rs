//   Copyright 2024 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use core::fmt;

pub trait MessageSpec {
    type Message: fmt::Debug + Send;
}
