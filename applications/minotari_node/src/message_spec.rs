// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use tari_core::proto;
use tari_network::MessageSpec;
use tari_p2p::message::DomainMessage;

pub enum TariNodeMessage {
    Request(proto::base_node::BaseNodeServiceRequest),
    Response(proto::base_node::BaseNodeServiceResponse),
    NewBlock(proto::core::NewBlock),
}

pub struct TariNodeMessageSpec;
impl MessageSpec for TariNodeMessageSpec {
    type Message = TariNodeMessage;
}
