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
    storage::{database::WalletBackend, memory_db::WalletMemoryDatabase},
    transaction_service::storage::{
        database::{CompletedTransaction, TransactionBackend, TransactionStatus},
        memory_db::TransactionMemoryDatabase,
    },
    wallet::WalletConfig,
    Wallet,
};
use chrono::{Duration as ChronoDuration, Utc};
use log::*;
use rand::{distributions::Alphanumeric, rngs::OsRng, CryptoRng, Rng, RngCore};
use std::{iter, sync::Arc, thread, time::Duration};
use tari_comms::{
    control_service::ControlServiceConfig,
    peer_manager::{NodeIdentity, PeerFeatures},
    types::{CommsPublicKey, CommsSecretKey},
};
use tari_comms_dht::DhtConfig;
use tari_core::transactions::{
    tari_amount::MicroTari,
    transaction::{OutputFeatures, Transaction, TransactionInput, UnblindedOutput},
    types::{BlindingFactor, CryptoFactories, PrivateKey, PublicKey},
};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    keys::{PublicKey as PublicKeyTrait, SecretKey as SecretKeyTrait},
    tari_utilities::hex::Hex,
};
use tari_p2p::initialization::CommsConfig;
use tari_test_utils::collect_stream;
use tokio::runtime::Runtime;

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
pub fn create_wallet(
    secret_key: CommsSecretKey,
    net_address: String,
    data_path: String,
) -> Wallet<WalletMemoryDatabase, TransactionMemoryDatabase, OutputManagerMemoryDatabase, ContactsServiceMemoryDatabase>
{
    let runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();

    let node_id = NodeIdentity::new(
        secret_key,
        net_address.as_str().parse().expect("Invalid Net Address"),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .expect("Could not construct Node Id");
    let comms_config = CommsConfig {
        node_identity: Arc::new(node_id.clone()),
        peer_connection_listening_address: "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
        socks_proxy_address: None,
        control_service: ControlServiceConfig {
            listening_address: node_id.public_address(),
            socks_proxy_address: None,
            public_peer_address: None,
            requested_connection_timeout: Duration::from_millis(500),
        },
        establish_connection_timeout: Duration::from_secs(2),
        datastore_path: data_path,
        peer_database_name: random_string(8),
        inbound_buffer_size: 100,
        outbound_buffer_size: 100,
        dht: DhtConfig {
            discovery_request_timeout: Duration::from_millis(500),
            ..Default::default()
        },
    };

    let config = WalletConfig {
        comms_config,
        logging_path: None,
        factories,
    };

    Wallet::new(
        config,
        runtime,
        WalletMemoryDatabase::new(),
        TransactionMemoryDatabase::new(),
        OutputManagerMemoryDatabase::new(),
        ContactsServiceMemoryDatabase::new(),
    )
    .expect("Could not create Wallet")
}

/// This function will generate a set of test data for the supplied wallet. Takes a few seconds to complete
pub fn generate_wallet_test_data<
    T: WalletBackend,
    U: TransactionBackend + Clone,
    V: OutputManagerBackend,
    W: ContactsBackend,
>(
    wallet: &mut Wallet<T, U, V, W>,
    data_path: &str,
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
    .map(|i| i.to_string())
    .collect();
    let mut message_index = 0;

    // attempt to avoid colliding ports for if two wallets are run on the same machine using this test data generation
    // function
    let random_port_offset = (OsRng.next_u64() % 100) as usize;

    // Generate contacts
    let mut generated_contacts = Vec::new();
    for i in 0..names.len() {
        let secret_key = CommsSecretKey::from_hex(private_keys[i]).expect("Could not parse hex key");
        let public_key = CommsPublicKey::from_secret_key(&secret_key);
        wallet
            .runtime
            .block_on(wallet.contacts_service.upsert_contact(Contact {
                alias: names[i].to_string(),
                public_key: public_key.clone(),
            }))?;

        wallet.set_base_node_peer(
            public_key.clone(),
            format!("/ip4/127.0.0.1/tcp/{}", 15200 + i + random_port_offset).to_string(),
        )?;
        generated_contacts.push((
            secret_key,
            format!("/ip4/127.0.0.1/tcp/{}", 15200 + i + random_port_offset).to_string(),
        ));
    }
    let contacts = wallet.runtime.block_on(wallet.contacts_service.get_contacts())?;
    assert_eq!(contacts.len(), names.len());
    info!(target: LOG_TARGET, "Added test contacts to wallet");

    // Generate outputs
    let num_outputs = 75;
    for i in 0..num_outputs {
        let (_ti, uo) = make_input(&mut OsRng.clone(), MicroTari::from(5_000_000 + i * 35_000), &factories);
        wallet.runtime.block_on(wallet.output_manager_service.add_output(uo))?;
    }
    info!(target: LOG_TARGET, "Added test outputs to wallet");
    // Generate some Tx history
    info!(
        target: LOG_TARGET,
        "Spinning up Alice wallet to generate test transactions"
    );
    let alice_temp_dir = format!("{}/{}", data_path.clone(), random_string(8));
    let _ = std::fs::create_dir(&alice_temp_dir);

    let mut wallet_alice = create_wallet(
        generated_contacts[0].0.clone(),
        generated_contacts[0].1.clone(),
        alice_temp_dir.clone(),
    );

    for i in 0..20 {
        let (_ti, uo) = make_input(&mut OsRng.clone(), MicroTari::from(1_500_000 + i * 530_500), &factories);
        wallet_alice
            .runtime
            .block_on(wallet_alice.output_manager_service.add_output(uo))?;
    }
    info!(target: LOG_TARGET, "Alice Wallet created");
    info!(
        target: LOG_TARGET,
        "Spinning up Bob wallet to generate test transactions"
    );
    let bob_temp_dir = format!("{}/{}", data_path.clone(), random_string(8));
    let _ = std::fs::create_dir(&bob_temp_dir);

    let mut wallet_bob = create_wallet(
        generated_contacts[1].0.clone(),
        generated_contacts[1].1.clone(),
        bob_temp_dir.clone(),
    );
    for i in 0..20 {
        let (_ti, uo) = make_input(
            &mut OsRng.clone(),
            MicroTari::from(2_000_000 + i * i * 61_050),
            &factories,
        );
        wallet_bob
            .runtime
            .block_on(wallet_bob.output_manager_service.add_output(uo))?;
    }
    info!(target: LOG_TARGET, "Bob Wallet created");

    info!(target: LOG_TARGET, "Starting to execute test transactions");
    // Completed TX
    wallet.runtime.block_on(wallet.transaction_service.send_transaction(
        contacts[0].public_key.clone(),
        MicroTari::from(1_100_000),
        MicroTari::from(100),
        messages[message_index].clone(),
    ))?;
    message_index = (message_index + 1) % messages.len();
    wallet.runtime.block_on(wallet.transaction_service.send_transaction(
        contacts[0].public_key.clone(),
        MicroTari::from(2_010_500),
        MicroTari::from(110),
        messages[message_index].clone(),
    ))?;
    message_index = (message_index + 1) % messages.len();

    wallet.runtime.block_on(wallet.transaction_service.send_transaction(
        contacts[0].public_key.clone(),
        MicroTari::from(10_000_000),
        MicroTari::from(110),
        messages[message_index].clone(),
    ))?;
    message_index = (message_index + 1) % messages.len();

    wallet.runtime.block_on(wallet.transaction_service.send_transaction(
        contacts[1].public_key.clone(),
        MicroTari::from(3_441_000),
        MicroTari::from(105),
        messages[message_index].clone(),
    ))?;
    message_index = (message_index + 1) % messages.len();

    wallet.runtime.block_on(wallet.transaction_service.send_transaction(
        contacts[1].public_key.clone(),
        MicroTari::from(14_100_000),
        MicroTari::from(100),
        messages[message_index].clone(),
    ))?;
    message_index = (message_index + 1) % messages.len();
    wallet.runtime.block_on(wallet.transaction_service.send_transaction(
        contacts[0].public_key.clone(),
        MicroTari::from(22_010_500),
        MicroTari::from(110),
        messages[message_index].clone(),
    ))?;
    message_index = (message_index + 1) % messages.len();

    wallet.runtime.block_on(wallet.transaction_service.send_transaction(
        contacts[0].public_key.clone(),
        MicroTari::from(17_000_000),
        MicroTari::from(110),
        messages[message_index].clone(),
    ))?;
    message_index = (message_index + 1) % messages.len();

    wallet.runtime.block_on(wallet.transaction_service.send_transaction(
        contacts[1].public_key.clone(),
        MicroTari::from(31_441_000),
        MicroTari::from(105),
        messages[message_index].clone(),
    ))?;
    message_index = (message_index + 1) % messages.len();

    wallet.runtime.block_on(wallet.transaction_service.send_transaction(
        contacts[0].public_key.clone(),
        MicroTari::from(12_100_000),
        MicroTari::from(100),
        messages[message_index].clone(),
    ))?;
    message_index = (message_index + 1) % messages.len();
    wallet.runtime.block_on(wallet.transaction_service.send_transaction(
        contacts[1].public_key.clone(),
        MicroTari::from(28_010_500),
        MicroTari::from(110),
        messages[message_index].clone(),
    ))?;
    message_index = (message_index + 1) % messages.len();

    // Pending Outbound
    wallet.runtime.block_on(wallet.transaction_service.send_transaction(
        contacts[2].public_key.clone(),
        MicroTari::from(2_500_000),
        MicroTari::from(107),
        messages[message_index].clone(),
    ))?;
    message_index = (message_index + 1) % messages.len();

    wallet.runtime.block_on(wallet.transaction_service.send_transaction(
        contacts[3].public_key.clone(),
        MicroTari::from(3_512_000),
        MicroTari::from(117),
        messages[message_index].clone(),
    ))?;
    message_index = (message_index + 1) % messages.len();

    // Make sure that the messages have been received by the alice and bob wallets before they start sending messages so
    // that they have the wallet in their peer_managers
    let alice_event_stream = wallet_alice.transaction_service.get_event_stream_fused();
    let bob_event_stream = wallet_bob.transaction_service.get_event_stream_fused();

    let _alice_stream = wallet_alice.runtime.block_on(async {
        collect_stream!(
            alice_event_stream.map(|i| (*i).clone()),
            take = 12,
            timeout = Duration::from_secs(60)
        )
    });

    let _bob_stream = wallet_bob.runtime.block_on(async {
        collect_stream!(
            bob_event_stream.map(|i| (*i).clone()),
            take = 8,
            timeout = Duration::from_secs(60)
        )
    });

    // Make sure that the messages have been received by the alice and bob wallets before they start sending messages so
    // that they have the wallet in their peer_managers
    let alice_event_stream = wallet_alice.transaction_service.get_event_stream_fused();
    let bob_event_stream = wallet_bob.transaction_service.get_event_stream_fused();

    let _alice_stream = wallet_bob.runtime.block_on(async {
        collect_stream!(
            alice_event_stream.map(|i| (*i).clone()),
            take = 6,
            timeout = Duration::from_secs(60)
        )
    });

    let _bob_stream = wallet_bob.runtime.block_on(async {
        collect_stream!(
            bob_event_stream.map(|i| (*i).clone()),
            take = 2,
            timeout = Duration::from_secs(60)
        )
    });

    // Pending Inbound
    wallet_alice
        .runtime
        .block_on(wallet_alice.transaction_service.send_transaction(
            wallet.comms.node_identity().public_key().clone(),
            MicroTari::from(1_235_000),
            MicroTari::from(117),
            messages[message_index].clone(),
        ))?;
    message_index = (message_index + 1) % messages.len();

    wallet_alice
        .runtime
        .block_on(wallet_alice.transaction_service.send_transaction(
            wallet.comms.node_identity().public_key().clone(),
            MicroTari::from(3_500_000),
            MicroTari::from(117),
            messages[message_index].clone(),
        ))?;
    message_index = (message_index + 1) % messages.len();

    wallet_alice
        .runtime
        .block_on(wallet_alice.transaction_service.send_transaction(
            wallet.comms.node_identity().public_key().clone(),
            MicroTari::from(2_335_000),
            MicroTari::from(117),
            messages[message_index].clone(),
        ))?;
    message_index = (message_index + 1) % messages.len();

    wallet_bob
        .runtime
        .block_on(wallet_bob.transaction_service.send_transaction(
            wallet.comms.node_identity().public_key().clone(),
            MicroTari::from(8_035_000),
            MicroTari::from(117),
            messages[message_index].clone(),
        ))?;
    message_index = (message_index + 1) % messages.len();

    wallet_bob
        .runtime
        .block_on(wallet_bob.transaction_service.send_transaction(
            wallet.comms.node_identity().public_key().clone(),
            MicroTari::from(5_135_000),
            MicroTari::from(117),
            messages[message_index].clone(),
        ))?;

    let wallet_event_stream = wallet.transaction_service.get_event_stream_fused();
    let _wallet_stream = wallet.runtime.block_on(async {
        collect_stream!(
            wallet_event_stream.map(|i| (*i).clone()),
            take = 20,
            timeout = Duration::from_secs(60)
        )
    });

    let txs = wallet
        .runtime
        .block_on(wallet.transaction_service.get_completed_transactions())
        .unwrap();

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
        let _ = transaction_service_backend
            .update_completed_transaction_timestamp((*k).clone(), timestamps[timestamp_index].clone());
        timestamp_index = (timestamp_index + 1) % timestamps.len();
    }

    let mut keys = Vec::new();

    for k in txs.keys().take(2) {
        keys.push(k);
    }

    // Broadcast a tx
    wallet
        .runtime
        .block_on(wallet.transaction_service.test_broadcast_transaction(keys[0].clone()))
        .unwrap();

    // Mine a tx
    wallet
        .runtime
        .block_on(wallet.transaction_service.test_mine_transaction(keys[1].clone()))
        .unwrap();

    thread::sleep(Duration::from_millis(1000));

    let _ = wallet_alice.shutdown();
    let _ = wallet_bob.shutdown();

    let _ = std::fs::remove_dir_all(&alice_temp_dir);
    let _ = std::fs::remove_dir_all(&bob_temp_dir);

    info!(target: LOG_TARGET, "Finished generating test data");

    Ok(())
}

/// This function is only available for testing and development by the client of LibWallet. It simulates a this node,
/// who sent a transaction out, accepting a reply to the Pending Outbound Transaction. That transaction then becomes a
/// CompletedTransaction with the Broadcast status indicating it is in a base node Mempool but not yet mined
pub fn complete_sent_transaction<
    T: WalletBackend,
    U: TransactionBackend + Clone,
    V: OutputManagerBackend,
    W: ContactsBackend,
>(
    wallet: &mut Wallet<T, U, V, W>,
    tx_id: TxId,
) -> Result<(), WalletError>
{
    let pending_outbound_tx = wallet
        .runtime
        .block_on(wallet.transaction_service.get_pending_outbound_transactions())?;
    match pending_outbound_tx.get(&tx_id) {
        Some(p) => {
            let completed_tx: CompletedTransaction = CompletedTransaction {
                tx_id: p.tx_id.clone(),
                source_public_key: wallet.comms.node_identity().public_key().clone(),
                destination_public_key: p.destination_public_key.clone(),
                amount: p.amount.clone(),
                fee: p.fee.clone(),
                transaction: Transaction::new(Vec::new(), Vec::new(), Vec::new(), BlindingFactor::default()),
                message: p.message.clone(),
                status: TransactionStatus::Completed,
                timestamp: Utc::now().naive_utc(),
            };
            wallet.runtime.block_on(
                wallet
                    .transaction_service
                    .test_complete_pending_transaction(completed_tx),
            )?;
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
pub fn receive_test_transaction<
    T: WalletBackend,
    U: TransactionBackend + Clone,
    V: OutputManagerBackend,
    W: ContactsBackend,
>(
    wallet: &mut Wallet<T, U, V, W>,
) -> Result<(), WalletError> {
    let contacts = wallet.runtime.block_on(wallet.contacts_service.get_contacts()).unwrap();
    let (_secret_key, mut public_key): (CommsSecretKey, CommsPublicKey) = PublicKey::random_keypair(&mut OsRng);

    if contacts.len() > 0 {
        public_key = contacts[0].public_key.clone();
    }

    wallet
        .runtime
        .block_on(wallet.transaction_service.test_accept_transaction(
            OsRng.next_u64(),
            MicroTari::from(10_000 + OsRng.next_u64() % 10_1000),
            public_key,
        ))?;

    Ok(())
}

/// This function is only available for testing and development by the client of LibWallet. It simulates this node,
/// who received a prior inbound transaction, accepting the Finalized Completed transaction from the Sender. That
/// transaction then becomes a CompletedTransaction with the Broadcast status indicating it is in a base node Mempool
/// but not yet mined
pub fn finalize_received_transaction<
    T: WalletBackend,
    U: TransactionBackend + Clone,
    V: OutputManagerBackend,
    W: ContactsBackend,
>(
    wallet: &mut Wallet<T, U, V, W>,
    tx_id: TxId,
) -> Result<(), WalletError>
{
    wallet
        .runtime
        .block_on(wallet.transaction_service.test_finalize_transaction(tx_id))?;

    Ok(())
}

/// This function is only available for testing and development by the client of LibWallet. This function will simulate
/// the event when a CompletedTransaction that is in the Complete status is broadcast to the Mempool and its status
/// moves to Broadcast. After this function is called the status of the CompletedTransaction becomes `Mined` and the
/// funds that were pending become spent and available respectively.
pub fn broadcast_transaction<
    T: WalletBackend,
    U: TransactionBackend + Clone,
    V: OutputManagerBackend,
    W: ContactsBackend,
>(
    wallet: &mut Wallet<T, U, V, W>,
    tx_id: TxId,
) -> Result<(), WalletError>
{
    wallet
        .runtime
        .block_on(wallet.transaction_service.test_broadcast_transaction(tx_id))?;

    Ok(())
}

/// This function is only available for testing and development by the client of LibWallet. This function will simulate
/// the event when a CompletedTransaction that is in the Broadcast status, is in a mempool but not mined, beocmes
/// mined/confirmed. After this function is called the status of the CompletedTransaction becomes `Mined` and the funds
/// that were pending become spent and available respectively.
pub fn mine_transaction<
    T: WalletBackend,
    U: TransactionBackend + Clone,
    V: OutputManagerBackend,
    W: ContactsBackend,
>(
    wallet: &mut Wallet<T, U, V, W>,
    tx_id: TxId,
) -> Result<(), WalletError>
{
    wallet
        .runtime
        .block_on(wallet.transaction_service.test_mine_transaction(tx_id))?;

    Ok(())
}
