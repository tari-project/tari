// Copyright 2020. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use crate::app::MyIdentity;
use chrono::{Duration as ChronoDuration, Utc};
use rand::{rngs::OsRng, RngCore};
use std::sync::atomic::{AtomicU64, Ordering};
use tari_core::transactions::{
    tari_amount::{uT, MicroTari},
    transaction::Transaction,
    types::{PrivateKey, PublicKey},
    ReceiverTransactionProtocol,
    SenderTransactionProtocol,
};
use tari_crypto::keys::{PublicKey as PublicKeyTrait, SecretKey as SecretKeyTrait};
use tari_wallet::{
    contacts_service::storage::database::Contact,
    transaction_service::storage::database::{
        CompletedTransaction,
        InboundTransaction,
        OutboundTransaction,
        TransactionDirection,
        TransactionStatus,
    },
};

pub fn dummy_inbound_txs() -> Vec<InboundTransaction> {
    let mut inbound_txs = Vec::new();

    inbound_txs.push(InboundTransaction {
        tx_id: 12342342323,
        source_public_key: PublicKey::random_keypair(&mut OsRng).1,
        amount: MicroTari::from(12345678902155),
        receiver_protocol: ReceiverTransactionProtocol::new_placeholder(),
        status: TransactionStatus::Pending,
        message: "For breakfast".to_string(),
        timestamp: Utc::now()
            .naive_utc()
            .checked_sub_signed(ChronoDuration::hours((OsRng.next_u64() % 11) as i64))
            .unwrap()
            .checked_sub_signed(ChronoDuration::minutes((OsRng.next_u64() % 59) as i64))
            .unwrap(),
        cancelled: false,
        direct_send_success: false,
    });

    inbound_txs.push(InboundTransaction {
        tx_id: 78874234324,
        source_public_key: PublicKey::random_keypair(&mut OsRng).1,
        amount: MicroTari::from(98765432345678),
        receiver_protocol: ReceiverTransactionProtocol::new_placeholder(),
        status: TransactionStatus::Pending,
        message: "For lunch".to_string(),
        timestamp: Utc::now()
            .naive_utc()
            .checked_sub_signed(ChronoDuration::hours((OsRng.next_u64() % 11) as i64))
            .unwrap()
            .checked_sub_signed(ChronoDuration::minutes((OsRng.next_u64() % 59) as i64))
            .unwrap(),
        cancelled: false,
        direct_send_success: false,
    });

    inbound_txs.push(InboundTransaction {
        tx_id: 8568345355467,
        source_public_key: PublicKey::random_keypair(&mut OsRng).1,
        amount: MicroTari::from(5547556434),
        receiver_protocol: ReceiverTransactionProtocol::new_placeholder(),
        status: TransactionStatus::Pending,
        message: "For dinner!".to_string(),
        timestamp: Utc::now()
            .naive_utc()
            .checked_sub_signed(ChronoDuration::hours((OsRng.next_u64() % 11) as i64))
            .unwrap()
            .checked_sub_signed(ChronoDuration::minutes((OsRng.next_u64() % 59) as i64))
            .unwrap(),
        cancelled: false,
        direct_send_success: false,
    });

    inbound_txs.sort_by(|a, b| b.timestamp.partial_cmp(&a.timestamp).unwrap());

    inbound_txs
}

pub fn dummy_outbound_txs() -> Vec<OutboundTransaction> {
    let mut outbound_txs = Vec::new();

    outbound_txs.push(OutboundTransaction {
        tx_id: 12342342323,
        destination_public_key: PublicKey::random_keypair(&mut OsRng).1,
        amount: MicroTari::from(12345678902155),
        fee: Default::default(),
        sender_protocol: SenderTransactionProtocol::new_placeholder(),
        status: TransactionStatus::Pending,
        message: "You know what its for".to_string(),
        timestamp: Utc::now()
            .naive_utc()
            .checked_sub_signed(ChronoDuration::hours((OsRng.next_u64() % 11) as i64))
            .unwrap()
            .checked_sub_signed(ChronoDuration::minutes((OsRng.next_u64() % 59) as i64))
            .unwrap(),
        cancelled: false,
        direct_send_success: false,
    });

    outbound_txs.push(OutboundTransaction {
        tx_id: 78678564453456,
        destination_public_key: PublicKey::random_keypair(&mut OsRng).1,
        amount: MicroTari::from(9876323235425354),
        fee: Default::default(),
        sender_protocol: SenderTransactionProtocol::new_placeholder(),
        status: TransactionStatus::Pending,
        message: "AHHHHHHHHHHHHHHHHhhHHhhhHHHHHHHhhhHHHHHHHHHHHHH!".to_string(),
        timestamp: Utc::now()
            .naive_utc()
            .checked_sub_signed(ChronoDuration::hours((OsRng.next_u64() % 11) as i64))
            .unwrap()
            .checked_sub_signed(ChronoDuration::minutes((OsRng.next_u64() % 59) as i64))
            .unwrap(),
        cancelled: false,
        direct_send_success: false,
    });
    outbound_txs.sort_by(|a, b| b.timestamp.partial_cmp(&a.timestamp).unwrap());
    outbound_txs
}

pub fn dummy_completed_txs() -> Vec<CompletedTransaction> {
    let mut completed_txs = Vec::new();
    let tx = Transaction::new(vec![], vec![], vec![], PrivateKey::random(&mut OsRng));
    completed_txs.push(CompletedTransaction {
        tx_id: OsRng.next_u64(),
        source_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        amount: OsRng.next_u64() % 999999999999 * uT,
        fee: MicroTari::from(100),
        transaction: tx.clone(),
        status: TransactionStatus::Completed,
        message: "Message one".to_string(),
        timestamp: Utc::now()
            .naive_utc()
            .checked_sub_signed(ChronoDuration::hours((OsRng.next_u64() % 11) as i64))
            .unwrap()
            .checked_sub_signed(ChronoDuration::minutes((OsRng.next_u64() % 59) as i64))
            .unwrap(),
        cancelled: false,
        direction: TransactionDirection::Outbound,
        coinbase_block_height: None,
    });

    completed_txs.push(CompletedTransaction {
        tx_id: OsRng.next_u64(),
        source_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        amount: OsRng.next_u64() % 999999999999 * uT,
        fee: MicroTari::from(100),
        transaction: tx.clone(),
        status: TransactionStatus::Completed,
        message: "Message two".to_string(),
        timestamp: Utc::now()
            .naive_utc()
            .checked_sub_signed(ChronoDuration::hours((OsRng.next_u64() % 11) as i64))
            .unwrap()
            .checked_sub_signed(ChronoDuration::minutes((OsRng.next_u64() % 59) as i64))
            .unwrap(),
        cancelled: false,
        direction: TransactionDirection::Inbound,
        coinbase_block_height: None,
    });

    completed_txs.push(CompletedTransaction {
        tx_id: OsRng.next_u64(),
        source_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        amount: OsRng.next_u64() % 999999999999 * uT,
        fee: MicroTari::from(100),
        transaction: tx.clone(),
        status: TransactionStatus::Broadcast,
        message: "Message three".to_string(),
        timestamp: Utc::now()
            .naive_utc()
            .checked_sub_signed(ChronoDuration::hours((OsRng.next_u64() % 11) as i64))
            .unwrap()
            .checked_sub_signed(ChronoDuration::minutes((OsRng.next_u64() % 59) as i64))
            .unwrap(),
        cancelled: false,
        direction: TransactionDirection::Inbound,
        coinbase_block_height: None,
    });

    completed_txs.push(CompletedTransaction {
        tx_id: OsRng.next_u64(),
        source_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        amount: OsRng.next_u64() % 999999999999 * uT,
        fee: MicroTari::from(100),
        transaction: tx.clone(),
        status: TransactionStatus::Broadcast,
        message: "Message four".to_string(),
        timestamp: Utc::now()
            .naive_utc()
            .checked_sub_signed(ChronoDuration::hours((OsRng.next_u64() % 11) as i64))
            .unwrap()
            .checked_sub_signed(ChronoDuration::minutes((OsRng.next_u64() % 59) as i64))
            .unwrap(),
        cancelled: false,
        direction: TransactionDirection::Outbound,
        coinbase_block_height: None,
    });

    completed_txs.push(CompletedTransaction {
        tx_id: OsRng.next_u64(),
        source_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        amount: OsRng.next_u64() % 999999999999 * uT,
        fee: MicroTari::from(100),
        transaction: tx.clone(),
        status: TransactionStatus::Broadcast,
        message: "Message five".to_string(),
        timestamp: Utc::now()
            .naive_utc()
            .checked_sub_signed(ChronoDuration::hours((OsRng.next_u64() % 11) as i64))
            .unwrap()
            .checked_sub_signed(ChronoDuration::minutes((OsRng.next_u64() % 59) as i64))
            .unwrap(),
        cancelled: false,
        direction: TransactionDirection::Outbound,
        coinbase_block_height: None,
    });

    completed_txs.push(CompletedTransaction {
        tx_id: OsRng.next_u64(),
        source_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        amount: OsRng.next_u64() % 999999999999 * uT,
        fee: MicroTari::from(100),
        transaction: tx.clone(),
        status: TransactionStatus::Mined,
        message: "Message six".to_string(),
        timestamp: Utc::now()
            .naive_utc()
            .checked_sub_signed(ChronoDuration::hours((OsRng.next_u64() % 11) as i64))
            .unwrap()
            .checked_sub_signed(ChronoDuration::minutes((OsRng.next_u64() % 59) as i64))
            .unwrap(),
        cancelled: false,
        direction: TransactionDirection::Inbound,
        coinbase_block_height: None,
    });

    completed_txs.push(CompletedTransaction {
        tx_id: OsRng.next_u64(),
        source_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        destination_public_key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        amount: OsRng.next_u64() % 999999999999 * uT,
        fee: MicroTari::from(100),
        transaction: tx,
        status: TransactionStatus::Mined,
        message: "Message seven".to_string(),
        timestamp: Utc::now()
            .naive_utc()
            .checked_sub_signed(ChronoDuration::hours((OsRng.next_u64() % 11) as i64))
            .unwrap()
            .checked_sub_signed(ChronoDuration::minutes((OsRng.next_u64() % 59) as i64))
            .unwrap(),
        cancelled: false,
        direction: TransactionDirection::Outbound,
        coinbase_block_height: None,
    });
    completed_txs.sort_by(|a, b| b.timestamp.partial_cmp(&a.timestamp).unwrap());

    completed_txs
}
lazy_static! {
    static ref BN_SYNC_CALLS: AtomicU64 = AtomicU64::new(0);
}

pub fn get_dummy_base_node_status() -> Option<u64> {
    let seconds = BN_SYNC_CALLS.fetch_add(1, Ordering::SeqCst) / 4;

    if seconds / 6 % 2 == 0 {
        None
    } else {
        Some(123456 + seconds / 10)
    }
}

pub fn get_dummy_identity() -> MyIdentity<'static> {
    MyIdentity {
        public_key: "92b34a4dc815531af8aeb8a1f1c8d18b927ddd7feabc706df6a1f87cf5014e54",
        public_address: "/onion3/mqsfoi62gonulivatrhitugwil3hcxf23eisaieetgyw7x2pdi2bzpyd:18142",
        emoji_id: "🐾💎🎤🎨📌🍄🎰🍉🚧💉💡👟🚒📌🔌🐶🐾🐢🔭🐨😻💨🐎🐊🚢👟🚧🐞🚜🌂🎩🎱📈",
        qr_code: "█▀▀▀▀▀█  ██ ▀▀▀ ██▀█▀█▀▄▀█▄▀▄ ▄▄▀ █▀▀▀▀▀█
    █ ███ █ █▄▀▄  ▀▀▀▀▀▄ █▀ █▀▄▀▄ █   █ ███ █
    █ ▀▀▀ █ █ ▀  ▄▄▀▀█▀▄▄███▄▀ ▀▄▀█▀▄ █ ▀▀▀ █
    ▀▀▀▀▀▀▀ █▄█ ▀ █▄▀ ▀ █▄█ █ █▄█ ▀▄▀ ▀▀▀▀▀▀▀
    ▀ ██▀█▀▄▄▄ ▄  ▀▄██▀▄ ▄▀█ █▀▀▄█▄█▄ ▀█▀█▀▄
    ▀▄█ ▄█▀ ▄█ ▄ ▄███▄▄ ▀▀▄ █▄ ▀██ ▄ █  ▀▄ ▄
    █ ▀▄ ▀▀██▄█ ▀█▄▀▄▀▄▀▀▀  ▀█▀█▀▄█ ██ ▄▄ ▀██
    ▀▀▀▀ █▄▄▄██▀█▄▄█ ▀▀ █▀▄▀█▀▄ ▀▄██▄█▀▀▄▀
    ▀▄ ▀██▀▄ █▀ ████▀   ▀▀█▀▄ ██ ▄▀ ▀█▄▀▄▀▀ ▄
    █▄▀▄▀▀█▄▀▄ █ ▄   ▀▀█ ▀▀▄ ▄▄██▄▀▀▄ ▀▀▀▄█▄
    ▄█▄▄▀█▀█  █▄ ▀ ▄▀▄▀ ▄█▀▀█▄▄█ ▄▄ ▀▄▄▀█  ▀▄
    ▄   ██▀▄█▀▄▄▄ ▄ ▄ ▀▀ ▄ ▀ ▀▀▄▀▀▄   ▀██▄
    ▀  ▀▀▄ █▄▀ ▀██▀▀▀██▄▀▄ ▀▀ █▀▄▄█▀▀▄▀  ▀
    ▀▄▄ █ ▀▄  █   ▀▄ ▄▀ █▀▀▄ ▄▄██▄▄▄█ ▀▄██ ▀▀
    ▄ ▄█▀█▀▀██▄███▀▄▄▄▀ ▄▀▀▀▄▄ █▄▄ ▀█▀▀▀▄█▀▄▀
    █ ▀▄▄ ▀▀▄█▄  ▄▀▀▄▄ █▄▀▄ ▄▄▀ ▄▄▀▀▄ █▀█▄ ▄▀
    ▀   ▀ ▀ ▄ ▀▄ ▄█  ▄█▀▄█   ▄▀▄  ▀▄█▀▀▀█ ██▄
    █▀▀▀▀▀█ ▄█▄   ▄▄ █▄▀▄█▄▀██▀▄█ ▀ █ ▀ █▄▄▄▄
    █ ███ █ ███▄█▄█ █▄▄ ▀▀▄ ▄ ▀▀  ▀█▀█▀▀▀▄▀█▄
    █ ▀▀▀ █ ▀█▄ ▄▄▄▄  █▀█▄▄▀  ▀▄ ▀▀▄ █▀▀▀███
    ▀▀▀▀▀▀▀ ▀ ▀           ▀ ▀▀ ▀ ▀    ▀ ▀▀\n",
    }
}

pub fn get_dummy_contacts() -> Vec<Contact> {
    let mut contacts = Vec::new();
    let names = [
        "Alice".to_string(),
        "Bob".to_string(),
        "Carol".to_string(),
        "Dave".to_string(),
        "Elvis".to_string(),
    ];
    for n in names.iter() {
        let (_secret_key, public_key) = PublicKey::random_keypair(&mut OsRng);

        contacts.push(Contact {
            alias: n.clone(),
            public_key,
        });
    }
    contacts
}
