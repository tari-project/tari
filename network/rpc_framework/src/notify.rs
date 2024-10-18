//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use libp2p_substream::ProtocolNotification;
use tokio::sync::mpsc;

/// Protocol notification receiver
pub type ProtocolNotificationRx<TSubstream> = mpsc::UnboundedReceiver<ProtocolNotification<TSubstream>>;
