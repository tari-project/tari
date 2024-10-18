// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

impl crate::proto::transaction_protocol::TransactionSenderMessage {
    pub fn none() -> Self {
        crate::proto::transaction_protocol::TransactionSenderMessage {
            message: Some(crate::proto::transaction_protocol::transaction_sender_message::Message::None(true)),
        }
    }

    pub fn single(data: crate::proto::transaction_protocol::SingleRoundSenderData) -> Self {
        crate::proto::transaction_protocol::TransactionSenderMessage {
            message: Some(crate::proto::transaction_protocol::transaction_sender_message::Message::Single(data)),
        }
    }

    pub fn multiple() -> Self {
        crate::proto::transaction_protocol::TransactionSenderMessage {
            message: Some(crate::proto::transaction_protocol::transaction_sender_message::Message::Multiple(true)),
        }
    }
}
