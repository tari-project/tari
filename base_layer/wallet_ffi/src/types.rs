//  Copyright 2022. The Tari Project
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

use libc::c_uchar;
use tokio::runtime::Runtime;
use minotari_wallet::WalletSqlite;
use tari_core::transactions::transaction_components::UnblindedOutput;
use tari_key_manager::SeedWords;
use tari_shutdown::Shutdown;
use crate::TariVector;

pub type TariTransportConfig = tari_p2p::TransportConfig;
pub type TariPublicKey = tari_common_types::types::PublicKey;
pub type TariWalletAddress = tari_common_types::tari_address::TariAddress;
pub type TariNodeId = tari_comms::peer_manager::NodeId;
pub type TariPrivateKey = tari_common_types::types::PrivateKey;
pub type TariOutputFeatures = tari_core::transactions::transaction_components::OutputFeatures;
pub type TariCommsConfig = tari_p2p::P2pConfig;
pub type TariTransactionKernel = tari_core::transactions::transaction_components::TransactionKernel;
pub type TariCovenant = tari_core::covenants::Covenant;
pub type TariEncryptedOpenings = tari_core::transactions::transaction_components::EncryptedData;
pub type TariComAndPubSignature = tari_common_types::types::ComAndPubSignature;
pub type TariUnblindedOutput = tari_core::transactions::transaction_components::UnblindedOutput;
pub type TariContact = tari_contacts::contacts_service::types::Contact;
pub type TariCompletedTransaction = minotari_wallet::transaction_service::storage::models::CompletedTransaction;
pub type TariTransactionSendStatus = minotari_wallet::transaction_service::handle::TransactionSendStatus;
pub type TariFeePerGramStats = minotari_wallet::transaction_service::handle::FeePerGramStatsResponse;
pub type TariFeePerGramStat = tari_core::mempool::FeePerGramStat;
pub type TariContactsLivenessData = tari_contacts::contacts_service::handle::ContactsLivenessData;
pub type TariBalance = minotari_wallet::output_manager_service::service::Balance;
pub type TariMnemonicLanguage = tari_key_manager::mnemonic::MnemonicLanguage;


pub struct TariUnblindedOutputs(pub(crate)Vec<UnblindedOutput>);

pub struct TariContacts(pub(crate) Vec<TariContact>);

pub struct TariCompletedTransactions(pub(crate) Vec<TariCompletedTransaction>);

pub type TariPendingInboundTransaction = minotari_wallet::transaction_service::storage::models::InboundTransaction;
pub type TariPendingOutboundTransaction = minotari_wallet::transaction_service::storage::models::OutboundTransaction;

pub struct TariPendingInboundTransactions(pub(crate) Vec<TariPendingInboundTransaction>);

pub struct TariPendingOutboundTransactions(pub(crate) Vec<TariPendingOutboundTransaction>);

#[derive(Debug, PartialEq, Clone)]
pub struct ByteVector(pub(crate)Vec<c_uchar>); // declared like this so that it can be exposed to external header

#[derive(Debug, PartialEq)]
pub struct EmojiSet(pub(crate)Vec<ByteVector>);

#[derive(Debug, PartialEq)]
pub struct TariSeedWords(pub(crate) SeedWords);

#[derive(Debug, PartialEq)]
pub struct TariPublicKeys(pub(crate) Vec<TariPublicKey>);

pub struct TariWallet {
    pub(crate) wallet: WalletSqlite,
    pub(crate) runtime: Runtime,
    pub(crate) shutdown: Shutdown,
}

#[derive(Debug)]
#[repr(C)]
pub struct TariCoinPreview {
    pub expected_outputs: *mut TariVector,
    pub fee: u64,
}

#[derive(Debug)]
#[repr(C)]
pub enum TariUtxoSort {
    ValueAsc = 0,
    ValueDesc = 1,
    MinedHeightAsc = 2,
    MinedHeightDesc = 3,
}
