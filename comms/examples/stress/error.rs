//  Copyright 2020, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE

use futures::channel::{mpsc::SendError, oneshot};
use std::io;
use tari_comms::{
    connectivity::ConnectivityError,
    peer_manager::{NodeIdentityError, PeerManagerError},
    tor,
    CommsBuilderError,
    PeerConnectionError,
};
use tari_crypto::tari_utilities::message_format::MessageFormatError;
use thiserror::Error;
use tokio::{task, time};

#[derive(Debug, Error)]
pub enum Error {
    #[error("NodeIdentityError: {0}")]
    NodeIdentityError(#[from] NodeIdentityError),
    #[error("HiddenServiceBuilderError: {0}")]
    HiddenServiceBuilderError(#[from] tor::HiddenServiceBuilderError),
    #[error("CommsBuilderError: {0}")]
    CommsBuilderError(#[from] CommsBuilderError),
    #[error("PeerManagerError: {0}")]
    PeerManagerError(#[from] PeerManagerError),
    #[error("Connection error: {0}")]
    PeerConnectionError(#[from] PeerConnectionError),
    #[error("Connectivity error: {0}")]
    ConnectivityError(#[from] ConnectivityError),
    #[error("Message format error: {0}")]
    MessageFormatError(#[from] MessageFormatError),
    #[error("Failed to send message")]
    SendError(#[from] SendError),
    #[error("JoinError: {0}")]
    JoinError(#[from] task::JoinError),
    #[error("Example did not exit cleanly: `{0}`")]
    WaitTimeout(#[from] time::Elapsed),
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("User quit")]
    UserQuit,
    #[error("Peer sent an invalid protocol frame")]
    InvalidProtocolFrame,
    #[error("Unexpected EoF")]
    UnexpectedEof,
    #[error("Internal reply canceled")]
    ReplyCanceled(#[from] oneshot::Canceled),
}
