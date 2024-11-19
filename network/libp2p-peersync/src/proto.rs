//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

include!(concat!(env!("OUT_DIR"), "/proto/mod.rs"));

pub use messages::*;

impl From<WantPeers> for Message {
    fn from(value: WantPeers) -> Self {
        Self {
            payload: mod_Message::OneOfpayload::WantPeers(value),
        }
    }
}

impl From<SignedPeerRecord> for Message {
    fn from(value: SignedPeerRecord) -> Self {
        Self {
            payload: mod_Message::OneOfpayload::LocalRecord(value),
        }
    }
}
