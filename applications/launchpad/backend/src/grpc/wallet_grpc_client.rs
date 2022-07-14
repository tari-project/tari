//  Copyright 2021. The Tari Project
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
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4},
    time::Duration,
};

use bollard::container::Stats;
use futures::{stream, Stream, StreamExt, TryStreamExt};
use log::info;
use tari_app_grpc::tari_rpc::{
    wallet_client::WalletClient,
    Empty,
    GetBalanceRequest,
    GetBalanceResponse,
    GetIdentityRequest,
    GetIdentityResponse,
    PaymentRecipient,
    TransactionEvent,
    TransactionEventRequest,
    TransactionEventResponse,
    TransferRequest,
};
use tonic::{transport::Channel, Request};

use super::{error::GrpcError, TransferFunds, TransferFundsResult};
use crate::{
    docker::{DockerWrapperError, LaunchpadConfig, WALLET_GRPC_ADDRESS_URL},
    error::LauncherError,
};

type Inner = WalletClient<tonic::transport::Channel>;

pub struct GrpcWalletClient {
    inner: Option<Inner>,
}

impl GrpcWalletClient {
    pub fn new() -> GrpcWalletClient {
        Self { inner: None }
    }

    pub async fn connection(&mut self) -> Result<&mut Inner, GrpcError> {
        if self.inner.is_none() {
            let inner = Inner::connect(WALLET_GRPC_ADDRESS_URL).await?;
            self.inner = Some(inner);
        }
        self.inner
            .as_mut()
            .ok_or_else(|| GrpcError::FatalError("no connection".into()))
    }

    pub async fn stream(&mut self) -> Result<impl Stream<Item = TransactionEventResponse>, GrpcError> {
        let inner = self.connection().await?;
        let request = TransactionEventRequest {};
        let response = inner.stream_transaction_events(request).await.unwrap().into_inner();
        Ok(response.map(|e| e.unwrap()))
    }

    pub async fn identity(&mut self) -> Result<GetIdentityResponse, GrpcError> {
        let inner = self.connection().await?;
        let request = GetIdentityRequest {};
        let identity = inner.identify(request).await?;
        Ok(identity.into_inner())
    }

    pub async fn balance(&mut self) -> Result<GetBalanceResponse, GrpcError> {
        let inner = self.connection().await?;
        let request = GetBalanceRequest {};
        let identity = inner.get_balance(request).await?;
        Ok(identity.into_inner())
    }

    pub async fn transfer_funds(&mut self, funds: TransferFunds) -> Result<TransferFundsResult, GrpcError> {
        let inner = self.connection().await?;
        let recipients: Vec<PaymentRecipient> = funds
            .payments
            .into_iter()
            .map(|p| PaymentRecipient {
                amount: p.amount,
                address: p.address,
                fee_per_gram: p.fee_per_gram,
                message: p.message,
                payment_type: p.payment_type,
            })
            .collect();

        let request = TransferRequest { recipients };
        let response = inner.transfer(request).await?.into_inner();
        Ok(TransferFundsResult::from(response))
    }

    pub async fn seed_words(&mut self) -> Result<Vec<String>, GrpcError> {
        let inner = self.connection().await?;
        let request = Empty {};
        let response = inner.seed_words(request).await?;
        Ok(response.into_inner().words)
    }

    pub async fn delete_seed_words(&mut self) -> Result<(), GrpcError> {
        let inner = self.connection().await?;
        let request = Empty {};
        let _accepted = inner.delete_seed_words_file(request).await?;
        Ok(())
    }
}
