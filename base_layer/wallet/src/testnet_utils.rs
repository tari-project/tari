// Copyright 2019. The Tari Project
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
use crate::{
    contacts_service::storage::{
        database::{Contact, ContactsBackend},
        memory_db::ContactsServiceMemoryDatabase,
    },
    error::{WalletError, WalletStorageError},
    output_manager_service::{
        storage::{database::OutputManagerBackend, memory_db::OutputManagerMemoryDatabase},
        TxId,
    },
    storage::{
        database::{DbKeyValuePair, WalletBackend, WriteOperation},
        memory_db::WalletMemoryDatabase,
    },
    test_utils::make_transaction_database,
    transaction_service::{
        handle::TransactionEvent,
        storage::{
            database::TransactionBackend,
            models::{CompletedTransaction, TransactionDirection, TransactionStatus},
            sqlite_db::TransactionServiceSqliteDatabase,
        },
    },
    wallet::WalletConfig,
    Wallet,
};
use chrono::{Duration as ChronoDuration, Utc};
use futures::{FutureExt, StreamExt};
use log::*;
use rand::{distributions::Alphanumeric, rngs::OsRng, CryptoRng, Rng, RngCore};
use std::{
    iter,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tari_common_types::chain_metadata::ChainMetadata;
use tari_comms::{
    multiaddr::Multiaddr,
    peer_manager::{NodeIdentity, PeerFeatures},
    transports::MemoryTransport,
    types::{CommsPublicKey, CommsSecretKey},
};
use tari_comms_dht::{envelope::Network as DhtNetwork, DhtConfig};
use tari_core::{
    consensus::Network,
    transactions::{
        tari_amount::MicroTari,
        transaction::{OutputFeatures, Transaction, TransactionInput, UnblindedOutput},
        types::{BlindingFactor, CryptoFactories, PrivateKey, PublicKey},
    },
};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    keys::{PublicKey as PublicKeyTrait, SecretKey as SecretKeyTrait},
    tari_utilities::hex::Hex,
};
use tari_p2p::{initialization::CommsConfig, transport::TransportType};
use tari_shutdown::{Shutdown, ShutdownSignal};
use tokio::{runtime::Handle, time::delay_for};

// Used to generate test wallet data

const LOG_TARGET: &str = "wallet::test_utils";

pub struct TestParams {
    pub spend_key: PrivateKey,
    pub change_key: PrivateKey,
    pub offset: PrivateKey,
    pub nonce: PrivateKey,
    pub public_nonce: PublicKey,
}
impl TestParams {
    pub fn new<R: Rng + CryptoRng>(rng: &mut R) -> TestParams {
        let r = PrivateKey::random(rng);
        TestParams {
            spend_key: PrivateKey::random(rng),
            change_key: PrivateKey::random(rng),
            offset: PrivateKey::random(rng),
            public_nonce: PublicKey::from_secret_key(&r),
            nonce: r,
        }
    }
}
pub fn make_input<R: Rng + CryptoRng>(
    rng: &mut R,
    val: MicroTari,
    factories: &CryptoFactories,
) -> (TransactionInput, UnblindedOutput)
{
    let key = PrivateKey::random(rng);
    let commitment = factories.commitment.commit_value(&key, val.into());
    let input = TransactionInput::new(OutputFeatures::default(), commitment);
    (input, UnblindedOutput::new(val, key, None))
}

pub fn random_string(len: usize) -> String {
    iter::repeat(()).map(|_| OsRng.sample(Alphanumeric)).take(len).collect()
}

/// Create a wallet for testing purposes
pub async fn create_wallet(
    secret_key: CommsSecretKey,
    public_address: Multiaddr,
    datastore_path: PathBuf,
    shutdown_signal: ShutdownSignal,
) -> Wallet<
    WalletMemoryDatabase,
    TransactionServiceSqliteDatabase,
    OutputManagerMemoryDatabase,
    ContactsServiceMemoryDatabase,
>
{
    let factories = CryptoFactories::default();

    let node_identity = Arc::new(
        NodeIdentity::new(secret_key, public_address.clone(), PeerFeatures::COMMUNICATION_NODE)
            .expect("Could not construct Node Identity"),
    );
    let comms_config = CommsConfig {
        transport_type: TransportType::Memory {
            listener_address: public_address,
        },
        node_identity,
        datastore_path: datastore_path.clone(),
        peer_database_name: random_string(8),
        max_concurrent_inbound_tasks: 100,
        outbound_buffer_size: 100,
        user_agent: "/tari/wallet/test".to_string(),
        dht: DhtConfig {
            discovery_request_timeout: Duration::from_secs(30),
            network: DhtNetwork::Stibbons,
            ..Default::default()
        },
        allow_test_addresses: true,
        listener_liveness_allowlist_cidrs: Vec::new(),
        listener_liveness_max_sessions: 0,
        dns_seeds: Default::default(),
        dns_seeds_name_server: "1.1.1.1:53".parse().unwrap(),
        dns_seeds_use_dnssec: false,
        peer_seeds: Default::default(),
    };

    let config = WalletConfig::new(comms_config, factories, None, None, Network::Stibbons, None, None, None);
    let db = WalletMemoryDatabase::new();
    let (backend, _) = make_transaction_database(Some(datastore_path.to_str().unwrap().to_string()));

    let metadata = ChainMetadata::new(std::u64::MAX, Vec::new(), 0, 0, 0);

    db.write(WriteOperation::Insert(DbKeyValuePair::BaseNodeChainMetadata(metadata)))
        .unwrap();
    Wallet::new(
        config,
        db,
        backend,
        OutputManagerMemoryDatabase::new(),
        ContactsServiceMemoryDatabase::new(),
        shutdown_signal,
    )
    .await
    .expect("Could not create Wallet")
}

pub fn get_next_memory_address() -> Multiaddr {
    let port = MemoryTransport::acquire_next_memsocket_port();
    format!("/memory/{}", port).parse().unwrap()
}

/// This function will generate a set of test data for the supplied wallet. Takes a few seconds to complete
pub async fn generate_wallet_test_data<
    T: WalletBackend,
    U: TransactionBackend,
    V: OutputManagerBackend,
    W: ContactsBackend,
    P: AsRef<Path>,
>(
    wallet: &mut Wallet<T, U, V, W>,
    data_path: P,
    transaction_service_backend: U,
) -> Result<(), WalletError>
{
    let factories = CryptoFactories::default();
    let names = ["Alice", "Bob", "Carol", "Dave"];
    let private_keys = [
        "3264e7a05ff669c1b71f691ab181ba3dd915306114a26c4a84c8da1dc1c40209",
        "fdad65858c7e7985168972f3117e31f7cee5a1d961fce690bd05a2a15ca6f00e",
        "07beb0d0d1eef08c246b70da8b060f7f8e885f5c0f2fd04b10607dc744b5f502",
        "bb2dcd0b477c8d709afe2547122a7199d6d4516bc6f35c2adb1a8afedbf97e0e",
    ];

    let messages: Vec<String> = vec![
        "My half of dinner",
        "Cheers",
        "April's rent",
        "Thanks for the Skywalker skin",
        "Here you go",
        "💰💰💰",
        "For the 'Tacos' 😉",
        "😀",
        "My share of the movie tickets",
        "Enjoy!",
        "😎",
        "Tickets!!",
        "For the cab fare",
        "👍👍",
        "🥡",
    ]
    .iter()
    .map(|i| (*i).to_string())
    .collect();
    let mut message_index = 0;

    let mut wallet_event_stream = wallet.transaction_service.get_event_stream_fused();

    // Generate contacts
    let mut generated_contacts = Vec::new();
    for i in 0..names.len() {
        let secret_key = CommsSecretKey::from_hex(private_keys[i]).expect("Could not parse hex key");
        let public_key = CommsPublicKey::from_secret_key(&secret_key);
        wallet
            .contacts_service
            .upsert_contact(Contact {
                alias: names[i].to_string(),
                public_key: public_key.clone(),
            })
            .await?;

        let addr = get_next_memory_address();
        generated_contacts.push((secret_key, addr));
    }
    let contacts = wallet.contacts_service.get_contacts().await?;
    assert_eq!(contacts.len(), names.len());
    info!(target: LOG_TARGET, "Added test contacts to wallet");

    // Generate outputs
    let num_outputs = 75;
    for i in 0..num_outputs {
        let (_ti, uo) = make_input(&mut OsRng.clone(), MicroTari::from(5_000_000 + i * 35_000), &factories);
        wallet.output_manager_service.add_output(uo).await?;
    }
    info!(target: LOG_TARGET, "Added test outputs to wallet");
    // Generate some Tx history
    info!(
        target: LOG_TARGET,
        "Spinning up Alice wallet to generate test transactions"
    );
    let alice_temp_dir = data_path.as_ref().join(random_string(8));
    let _ = std::fs::create_dir(&alice_temp_dir);

    let mut shutdown_a = Shutdown::new();
    let mut shutdown_b = Shutdown::new();
    let mut wallet_alice = create_wallet(
        generated_contacts[0].0.clone(),
        generated_contacts[0].1.clone(),
        alice_temp_dir.clone(),
        shutdown_a.to_signal(),
    )
    .await;
    let mut alice_event_stream = wallet_alice.transaction_service.get_event_stream_fused();
    for i in 0..20 {
        let (_ti, uo) = make_input(&mut OsRng.clone(), MicroTari::from(1_500_000 + i * 530_500), &factories);
        wallet_alice.output_manager_service.add_output(uo).await?;
    }
    info!(target: LOG_TARGET, "Alice Wallet created");
    info!(
        target: LOG_TARGET,
        "Spinning up Bob wallet to generate test transactions"
    );
    let bob_temp_dir = data_path.as_ref().join(random_string(8));
    let _ = std::fs::create_dir(&bob_temp_dir);

    let mut wallet_bob = create_wallet(
        generated_contacts[1].0.clone(),
        generated_contacts[1].1.clone(),
        bob_temp_dir.clone(),
        shutdown_b.to_signal(),
    )
    .await;
    let mut bob_event_stream = wallet_bob.transaction_service.get_event_stream_fused();

    for i in 0..20 {
        let (_ti, uo) = make_input(
            &mut OsRng.clone(),
            MicroTari::from(2_000_000 + i * i * 61_050),
            &factories,
        );
        wallet_bob.output_manager_service.add_output(uo).await?;
    }
    info!(target: LOG_TARGET, "Bob Wallet created");

    let alice_peer = wallet_alice.comms.node_identity().to_peer();

    wallet.comms.peer_manager().add_peer(alice_peer).await?;

    let bob_peer = wallet_bob.comms.node_identity().to_peer();

    wallet.comms.peer_manager().add_peer(bob_peer).await?;

    wallet
        .comms
        .connectivity()
        .dial_peer(wallet_alice.comms.node_identity().node_id().clone())
        .await
        .unwrap();

    wallet
        .comms
        .connectivity()
        .dial_peer(wallet_bob.comms.node_identity().node_id().clone())
        .await
        .unwrap();
    info!(target: LOG_TARGET, "Starting to execute test transactions");

    // Grab the first 2 outbound tx_ids for later
    let mut outbound_tx_ids = Vec::new();

    // Completed TX
    let tx_id = wallet
        .transaction_service
        .send_transaction(
            contacts[0].public_key.clone(),
            MicroTari::from(1_100_000),
            MicroTari::from(100),
            messages[message_index].clone(),
        )
        .await?;
    outbound_tx_ids.push(tx_id);
    message_index = (message_index + 1) % messages.len();

    let tx_id = wallet
        .transaction_service
        .send_transaction(
            contacts[0].public_key.clone(),
            MicroTari::from(2_010_500),
            MicroTari::from(110),
            messages[message_index].clone(),
        )
        .await?;
    outbound_tx_ids.push(tx_id);
    message_index = (message_index + 1) % messages.len();

    let mut delay = delay_for(Duration::from_secs(60)).fuse();
    let mut count = 0;
    loop {
        futures::select! {
            event = alice_event_stream.select_next_some() => {
                match &*event.unwrap() {
                    TransactionEvent::ReceivedTransaction(_) => {
                        count +=1;
                    },
                    TransactionEvent::ReceivedFinalizedTransaction(_) => {
                        count +=1;
                    },
                    _ => (),
                }
                if count >=4 {
                    break;
                }
            },
            () = delay => {
                break;
            },
        }
    }
    assert!(count >= 4, "Event waiting timed out before receiving expected events 1");

    wallet
        .transaction_service
        .send_transaction(
            contacts[0].public_key.clone(),
            MicroTari::from(10_000_000),
            MicroTari::from(110),
            messages[message_index].clone(),
        )
        .await?;
    message_index = (message_index + 1) % messages.len();

    wallet
        .transaction_service
        .send_transaction(
            contacts[1].public_key.clone(),
            MicroTari::from(3_441_000),
            MicroTari::from(105),
            messages[message_index].clone(),
        )
        .await?;
    message_index = (message_index + 1) % messages.len();

    wallet
        .transaction_service
        .send_transaction(
            contacts[1].public_key.clone(),
            MicroTari::from(14_100_000),
            MicroTari::from(100),
            messages[message_index].clone(),
        )
        .await?;
    message_index = (message_index + 1) % messages.len();
    wallet
        .transaction_service
        .send_transaction(
            contacts[0].public_key.clone(),
            MicroTari::from(22_010_500),
            MicroTari::from(110),
            messages[message_index].clone(),
        )
        .await?;
    message_index = (message_index + 1) % messages.len();

    wallet
        .transaction_service
        .send_transaction(
            contacts[0].public_key.clone(),
            MicroTari::from(17_000_000),
            MicroTari::from(110),
            messages[message_index].clone(),
        )
        .await?;
    message_index = (message_index + 1) % messages.len();

    wallet
        .transaction_service
        .send_transaction(
            contacts[1].public_key.clone(),
            MicroTari::from(31_441_000),
            MicroTari::from(105),
            messages[message_index].clone(),
        )
        .await?;
    message_index = (message_index + 1) % messages.len();

    wallet
        .transaction_service
        .send_transaction(
            contacts[0].public_key.clone(),
            MicroTari::from(12_100_000),
            MicroTari::from(100),
            messages[message_index].clone(),
        )
        .await?;
    message_index = (message_index + 1) % messages.len();
    wallet
        .transaction_service
        .send_transaction(
            contacts[1].public_key.clone(),
            MicroTari::from(28_010_500),
            MicroTari::from(110),
            messages[message_index].clone(),
        )
        .await?;
    message_index = (message_index + 1) % messages.len();

    // Pending Outbound
    let _ = wallet
        .transaction_service
        .send_transaction(
            contacts[2].public_key.clone(),
            MicroTari::from(2_500_000),
            MicroTari::from(107),
            messages[message_index].clone(),
        )
        .await;
    message_index = (message_index + 1) % messages.len();

    let _ = wallet
        .transaction_service
        .send_transaction(
            contacts[3].public_key.clone(),
            MicroTari::from(3_512_000),
            MicroTari::from(117),
            messages[message_index].clone(),
        )
        .await;
    message_index = (message_index + 1) % messages.len();

    let mut delay = delay_for(Duration::from_secs(60)).fuse();
    let mut count = 0;
    loop {
        futures::select! {
            event = wallet_event_stream.select_next_some() => {
                if let TransactionEvent::TransactionDirectSendResult(_,_) = &*event.unwrap() {
                    count+=1;
                    if count >= 10 {
                        break;
                    }
                }
            },
            () = delay => {
                break;
            },
        }
    }
    assert!(
        count >= 10,
        "Event waiting timed out before receiving expected events 2"
    );

    let mut delay = delay_for(Duration::from_secs(60)).fuse();
    let mut count = 0;
    loop {
        futures::select! {
            event = bob_event_stream.select_next_some() => {
                match &*event.unwrap() {
                    TransactionEvent::ReceivedTransaction(_) => {
                        count+=1;
                    },
                    TransactionEvent::ReceivedFinalizedTransaction(_) => {
                        count+=1;
                    },
                    _ => (),
                }
                if count >= 8 {
                    break;
                }
            },
            () = delay => {
                break;
            },
        }
    }
    assert!(count >= 8, "Event waiting timed out before receiving expected events 3");

    log::error!("Inbound Transactions starting");
    // Pending Inbound
    wallet_alice
        .transaction_service
        .send_transaction(
            wallet.comms.node_identity().public_key().clone(),
            MicroTari::from(1_235_000),
            MicroTari::from(117),
            messages[message_index].clone(),
        )
        .await?;
    message_index = (message_index + 1) % messages.len();

    wallet_alice
        .transaction_service
        .send_transaction(
            wallet.comms.node_identity().public_key().clone(),
            MicroTari::from(3_500_000),
            MicroTari::from(117),
            messages[message_index].clone(),
        )
        .await?;
    message_index = (message_index + 1) % messages.len();

    wallet_alice
        .transaction_service
        .send_transaction(
            wallet.comms.node_identity().public_key().clone(),
            MicroTari::from(2_335_000),
            MicroTari::from(117),
            messages[message_index].clone(),
        )
        .await?;
    message_index = (message_index + 1) % messages.len();

    wallet_bob
        .transaction_service
        .send_transaction(
            wallet.comms.node_identity().public_key().clone(),
            MicroTari::from(8_035_000),
            MicroTari::from(117),
            messages[message_index].clone(),
        )
        .await?;
    message_index = (message_index + 1) % messages.len();

    wallet_bob
        .transaction_service
        .send_transaction(
            wallet.comms.node_identity().public_key().clone(),
            MicroTari::from(5_135_000),
            MicroTari::from(117),
            messages[message_index].clone(),
        )
        .await?;

    let mut delay = delay_for(Duration::from_secs(60)).fuse();
    let mut count = 0;
    loop {
        futures::select! {
            event = wallet_event_stream.select_next_some() => {
                if let TransactionEvent::ReceivedFinalizedTransaction(_) = &*event.unwrap() {
                    count+=1;
                    if count >= 5 {
                        break;
                    }
                }
            },
            () = delay => {
                break;
            },
        }
    }
    assert!(count >= 5, "Event waiting timed out before receiving expected events 4");

    let txs = wallet.transaction_service.get_completed_transactions().await.unwrap();

    let timestamps = vec![
        Utc::now()
            .naive_utc()
            .checked_sub_signed(ChronoDuration::seconds(60))
            .unwrap(),
        Utc::now()
            .naive_utc()
            .checked_sub_signed(ChronoDuration::minutes(5))
            .unwrap(),
        Utc::now()
            .naive_utc()
            .checked_sub_signed(ChronoDuration::minutes(11))
            .unwrap(),
        Utc::now()
            .naive_utc()
            .checked_sub_signed(ChronoDuration::hours(2))
            .unwrap(),
        Utc::now()
            .naive_utc()
            .checked_sub_signed(ChronoDuration::hours(3))
            .unwrap(),
        Utc::now()
            .naive_utc()
            .checked_sub_signed(ChronoDuration::hours(8))
            .unwrap(),
        Utc::now()
            .naive_utc()
            .checked_sub_signed(ChronoDuration::hours(27))
            .unwrap(),
        Utc::now()
            .naive_utc()
            .checked_sub_signed(ChronoDuration::hours(34))
            .unwrap(),
        Utc::now()
            .naive_utc()
            .checked_sub_signed(ChronoDuration::hours(51))
            .unwrap(),
        Utc::now()
            .naive_utc()
            .checked_sub_signed(ChronoDuration::hours(59))
            .unwrap(),
        Utc::now()
            .naive_utc()
            .checked_sub_signed(ChronoDuration::days(9))
            .unwrap()
            .checked_sub_signed(ChronoDuration::hours(3))
            .unwrap(),
        Utc::now()
            .naive_utc()
            .checked_sub_signed(ChronoDuration::days(10))
            .unwrap()
            .checked_sub_signed(ChronoDuration::hours(6))
            .unwrap(),
        Utc::now()
            .naive_utc()
            .checked_sub_signed(ChronoDuration::days(12))
            .unwrap()
            .checked_sub_signed(ChronoDuration::hours(2))
            .unwrap(),
        Utc::now()
            .naive_utc()
            .checked_sub_signed(ChronoDuration::days(15))
            .unwrap()
            .checked_sub_signed(ChronoDuration::hours(2))
            .unwrap(),
        Utc::now()
            .naive_utc()
            .checked_sub_signed(ChronoDuration::days(16))
            .unwrap()
            .checked_sub_signed(ChronoDuration::hours(2))
            .unwrap(),
    ];
    let mut timestamp_index = 0;

    for k in txs.keys() {
        let _ = transaction_service_backend.update_completed_transaction_timestamp(*k, timestamps[timestamp_index]);
        timestamp_index = (timestamp_index + 1) % timestamps.len();
    }

    // Broadcast a tx

    wallet
        .transaction_service
        .test_broadcast_transaction(outbound_tx_ids[0])
        .await
        .unwrap();

    // Mine a tx
    wallet
        .transaction_service
        .test_mine_transaction(outbound_tx_ids[1])
        .await
        .unwrap();

    delay_for(Duration::from_secs(1)).await;

    shutdown_a.trigger().unwrap();
    shutdown_b.trigger().unwrap();
    wallet_alice.wait_until_shutdown().await;
    wallet_bob.wait_until_shutdown().await;

    let _ = std::fs::remove_dir_all(&alice_temp_dir);
    let _ = std::fs::remove_dir_all(&bob_temp_dir);

    info!(target: LOG_TARGET, "Finished generating test data");

    Ok(())
}

/// This function is only available for testing and development by the client of LibWallet. It simulates a this node,
/// who sent a transaction out, accepting a reply to the Pending Outbound Transaction. That transaction then becomes a
/// CompletedTransaction with the Broadcast status indicating it is in a base node Mempool but not yet mined
pub async fn complete_sent_transaction<
    T: WalletBackend,
    U: TransactionBackend,
    V: OutputManagerBackend,
    W: ContactsBackend,
>(
    wallet: &mut Wallet<T, U, V, W>,
    tx_id: TxId,
) -> Result<(), WalletError>
{
    let pending_outbound_tx = wallet.transaction_service.get_pending_outbound_transactions().await?;
    match pending_outbound_tx.get(&tx_id) {
        Some(p) => {
            let completed_tx: CompletedTransaction = CompletedTransaction::new(
                p.tx_id,
                wallet.comms.node_identity().public_key().clone(),
                p.destination_public_key.clone(),
                p.amount,
                p.fee,
                Transaction::new(Vec::new(), Vec::new(), Vec::new(), BlindingFactor::default()),
                TransactionStatus::Completed,
                p.message.clone(),
                Utc::now().naive_utc(),
                TransactionDirection::Outbound,
                None,
            );

            wallet
                .transaction_service
                .test_complete_pending_transaction(completed_tx)
                .await?;
        },
        None => {
            return Err(WalletError::WalletStorageError(WalletStorageError::UnexpectedResult(
                "Pending outbound transaction does not exist".to_string(),
            )))
        },
    }

    Ok(())
}

/// This function is only available for testing by the client of LibWallet. This function simulates an external
/// wallet sending a transaction to this wallet which will become a PendingInboundTransaction
pub async fn receive_test_transaction<
    T: WalletBackend,
    U: TransactionBackend,
    V: OutputManagerBackend,
    W: ContactsBackend,
>(
    wallet: &mut Wallet<T, U, V, W>,
    handle: &Handle,
) -> Result<(), WalletError>
{
    let contacts = wallet.contacts_service.get_contacts().await.unwrap();
    let (_secret_key, mut public_key): (CommsSecretKey, CommsPublicKey) = PublicKey::random_keypair(&mut OsRng);

    if !contacts.is_empty() {
        public_key = contacts[0].public_key.clone();
    }

    wallet
        .transaction_service
        .test_accept_transaction(
            OsRng.next_u64(),
            MicroTari::from(10_000 + OsRng.next_u64() % 101_000),
            public_key,
            handle,
        )
        .await?;

    Ok(())
}

/// This function is only available for testing and development by the client of LibWallet. It simulates this node,
/// who received a prior inbound transaction, accepting the Finalized Completed transaction from the Sender. That
/// transaction then becomes a CompletedTransaction with the Broadcast status indicating it is in a base node Mempool
/// but not yet mined
pub async fn finalize_received_transaction<
    T: WalletBackend,
    U: TransactionBackend,
    V: OutputManagerBackend,
    W: ContactsBackend,
>(
    wallet: &mut Wallet<T, U, V, W>,
    tx_id: TxId,
) -> Result<(), WalletError>
{
    wallet.transaction_service.test_finalize_transaction(tx_id).await?;

    Ok(())
}

/// This function is only available for testing and development by the client of LibWallet. This function will simulate
/// the event when a CompletedTransaction that is in the Complete status is broadcast to the Mempool and its status
/// moves to Broadcast. After this function is called the status of the CompletedTransaction becomes `Mined` and the
/// funds that were pending become spent and available respectively.
pub async fn broadcast_transaction<
    T: WalletBackend,
    U: TransactionBackend,
    V: OutputManagerBackend,
    W: ContactsBackend,
>(
    wallet: &mut Wallet<T, U, V, W>,
    tx_id: TxId,
) -> Result<(), WalletError>
{
    wallet.transaction_service.test_broadcast_transaction(tx_id).await?;

    Ok(())
}

/// This function is only available for testing and development by the client of LibWallet. This function will simulate
/// the event when a CompletedTransaction that is in the Broadcast status, is in a mempool but not mined, beocmes
/// mined/confirmed. After this function is called the status of the CompletedTransaction becomes `Mined` and the funds
/// that were pending become spent and available respectively.
pub async fn mine_transaction<T: WalletBackend, U: TransactionBackend, V: OutputManagerBackend, W: ContactsBackend>(
    wallet: &mut Wallet<T, U, V, W>,
    tx_id: TxId,
) -> Result<(), WalletError>
{
    wallet.transaction_service.test_mine_transaction(tx_id).await?;

    Ok(())
}
