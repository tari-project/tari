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
    contacts_service::storage::database::Contact,
    error::{WalletError, WalletStorageError},
    output_manager_service::TxId,
    storage::{database::WalletBackend, memory_db::WalletMemoryDatabase},
    transaction_service::storage::database::{CompletedTransaction, TransactionStatus},
    wallet::WalletConfig,
    Wallet,
};
use chrono::Utc;
use rand::{distributions::Alphanumeric, CryptoRng, OsRng, Rng, RngCore};
use std::{iter, sync::Arc, thread, time::Duration};
use tari_comms::{
    control_service::ControlServiceConfig,
    peer_manager::{NodeIdentity, PeerFeatures},
    types::{CommsPublicKey, CommsSecretKey},
};
use tari_comms_dht::DhtConfig;
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    keys::{PublicKey as PublicKeyTrait, SecretKey as SecretKeyTrait},
};
use tari_p2p::initialization::CommsConfig;
use tari_transactions::{
    tari_amount::MicroTari,
    transaction::{OutputFeatures, Transaction, TransactionInput, UnblindedOutput},
    types::{BlindingFactor, CryptoFactories, PrivateKey, PublicKey},
};
use tari_utilities::hex::Hex;
use tempdir::TempDir;
use tokio::runtime::Runtime;

// Used to generate test wallet data

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
    let mut rng = OsRng::new().unwrap();
    iter::repeat(()).map(|_| rng.sample(Alphanumeric)).take(len).collect()
}

/// Create a wallet for testing purposes
pub fn create_wallet(secret_key: CommsSecretKey, net_address: String) -> Wallet<WalletMemoryDatabase> {
    let runtime = Runtime::new().unwrap();
    let factories = CryptoFactories::default();

    let node_id = NodeIdentity::new(
        secret_key,
        net_address.as_str().parse().expect("Invalid Net Address"),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .expect("Could not construct Node Id");
    let mut dht_config: DhtConfig = Default::default();
    dht_config.discovery_request_timeout = Duration::from_millis(500);
    let comms_config = CommsConfig {
        node_identity: Arc::new(node_id.clone()),
        peer_connection_listening_address: "127.0.0.1".parse().unwrap(),
        socks_proxy_address: None,
        control_service: ControlServiceConfig {
            listener_address: node_id.control_service_address(),
            socks_proxy_address: None,
            requested_connection_timeout: Duration::from_millis(500),
        },
        establish_connection_timeout: Duration::from_secs(2),
        datastore_path: TempDir::new(random_string(8).as_str())
            .unwrap()
            .path()
            .to_str()
            .unwrap()
            .to_string(),
        peer_database_name: random_string(8),
        inbound_buffer_size: 100,
        outbound_buffer_size: 100,
        dht: dht_config,
    };

    let config = WalletConfig {
        comms_config,
        factories,
    };

    Wallet::new(config, WalletMemoryDatabase::new(), runtime).expect("Could not create Wallet")
}

/// This function will generate a set of test data for the supplied wallet. Takes a few seconds to complete
pub fn generate_wallet_test_data<T: WalletBackend>(wallet: &mut Wallet<T>) -> Result<(), WalletError> {
    let mut rng = rand::OsRng::new().unwrap();
    let factories = CryptoFactories::default();
    let names = ["Alice", "Bob", "Carol", "Dave"];
    let private_keys = [
        "3264e7a05ff669c1b71f691ab181ba3dd915306114a26c4a84c8da1dc1c40209",
        "fdad65858c7e7985168972f3117e31f7cee5a1d961fce690bd05a2a15ca6f00e",
        "07beb0d0d1eef08c246b70da8b060f7f8e885f5c0f2fd04b10607dc744b5f502",
        "bb2dcd0b477c8d709afe2547122a7199d6d4516bc6f35c2adb1a8afedbf97e0e",
    ];
    // Generate contacts
    let mut generated_contacts = Vec::new();
    for i in 0..names.len() {
        let secret_key = CommsSecretKey::from_hex(private_keys[i]).unwrap();
        let public_key = CommsPublicKey::from_secret_key(&secret_key);
        wallet
            .runtime
            .block_on(wallet.contacts_service.save_contact(Contact {
                alias: names[i].to_string(),
                public_key: public_key.clone(),
            }))
            .expect("Could not save contact");
        wallet
            .add_base_node_peer(public_key.clone(), format!("127.0.0.1:{}", 15200 + i).to_string())
            .expect("Could not add base node peer");
        generated_contacts.push((secret_key, format!("127.0.0.1:{}", 15200 + i).to_string()));
    }
    let contacts = wallet
        .runtime
        .block_on(wallet.contacts_service.get_contacts())
        .expect("Could not retrieve contacts");
    assert_eq!(contacts.len(), names.len());

    // Generate outputs
    let num_outputs = 40;
    for i in 0..num_outputs {
        let (_ti, uo) = make_input(&mut rng.clone(), MicroTari::from(1_000_000 + i * 35_000), &factories);
        wallet
            .runtime
            .block_on(wallet.output_manager_service.add_output(uo))
            .unwrap();
    }

    // Generate some Tx history
    let mut wallet_alice = create_wallet(generated_contacts[0].0.clone(), generated_contacts[0].1.clone());
    for i in 0..20 {
        let (_ti, uo) = make_input(&mut rng.clone(), MicroTari::from(1_500_000 + i * 530_500), &factories);
        wallet_alice
            .runtime
            .block_on(wallet_alice.output_manager_service.add_output(uo))
            .unwrap();
    }

    let mut wallet_bob = create_wallet(generated_contacts[1].0.clone(), generated_contacts[1].1.clone());
    for i in 0..20 {
        let (_ti, uo) = make_input(
            &mut rng.clone(),
            MicroTari::from(2_000_000 + i * i * 61_050),
            &factories,
        );
        wallet_bob
            .runtime
            .block_on(wallet_bob.output_manager_service.add_output(uo))
            .unwrap();
    }

    // Completed TX
    wallet
        .runtime
        .block_on(wallet.transaction_service.send_transaction(
            contacts[0].public_key.clone(),
            MicroTari::from(10_000),
            MicroTari::from(100),
            "".to_string(),
        ))
        .expect("Could not send test transaction");

    wallet
        .runtime
        .block_on(wallet.transaction_service.send_transaction(
            contacts[0].public_key.clone(),
            MicroTari::from(20_000),
            MicroTari::from(110),
            "".to_string(),
        ))
        .expect("Could not send test transaction");

    wallet
        .runtime
        .block_on(wallet.transaction_service.send_transaction(
            contacts[1].public_key.clone(),
            MicroTari::from(30_000),
            MicroTari::from(105),
            "".to_string(),
        ))
        .expect("Could not send test transaction");

    // Pending Outbound
    wallet
        .runtime
        .block_on(wallet.transaction_service.send_transaction(
            contacts[2].public_key.clone(),
            MicroTari::from(25_000),
            MicroTari::from(107),
            "".to_string(),
        ))
        .expect("Could not send test transaction");

    wallet
        .runtime
        .block_on(wallet.transaction_service.send_transaction(
            contacts[3].public_key.clone(),
            MicroTari::from(35_000),
            MicroTari::from(117),
            "".to_string(),
        ))
        .expect("Could not send test transaction");

    // Pending Inbound
    wallet_alice
        .runtime
        .block_on(wallet_alice.transaction_service.send_transaction(
            wallet.comms.node_identity().public_key().clone(),
            MicroTari::from(35_000),
            MicroTari::from(117),
            "".to_string(),
        ))
        .expect("Could not send test transaction");
    wallet_bob
        .runtime
        .block_on(wallet_bob.transaction_service.send_transaction(
            wallet.comms.node_identity().public_key().clone(),
            MicroTari::from(35_000),
            MicroTari::from(117),
            "".to_string(),
        ))
        .expect("Could not send test transaction");

    thread::sleep(Duration::from_millis(1000));

    Ok(())
}

/// This function is only available for testing and development by the client of LibWallet. It simulates a this node,
/// who sent a transaction out, accepting a reply to the Pending Outbound Transaction. That transaction then becomes a
/// CompletedTransaction with the Broadcast status indicating it is in a base node Mempool but not yet mined
pub fn complete_sent_transaction<T: WalletBackend>(wallet: &mut Wallet<T>, tx_id: TxId) -> Result<(), WalletError> {
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
                status: TransactionStatus::Broadcast,
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
pub fn receive_test_transaction<T: WalletBackend>(wallet: &mut Wallet<T>) -> Result<(), WalletError> {
    let mut rng = OsRng::new().unwrap();
    let (_secret_key, public_key): (CommsSecretKey, CommsPublicKey) = PublicKey::random_keypair(&mut rng);

    wallet
        .runtime
        .block_on(wallet.transaction_service.test_accept_transaction(
            rng.next_u64(),
            MicroTari::from(10_000 + rng.next_u64() % 10_1000),
            public_key,
        ))?;

    Ok(())
}

pub fn broadcast_transaction<T: WalletBackend>(wallet: &mut Wallet<T>, tx_id: TxId) -> Result<(), WalletError> {
    wallet
        .runtime
        .block_on(wallet.transaction_service.test_broadcast_transaction(tx_id))?;

    Ok(())
}

/// This function is only available for testing and development by the client of LibWallet. This function will simulate
/// the event when a CompletedTransaction that is in the Broadcast status, is in a mempool but not mined, beocmes
/// mined/confirmed. After this function is called the status of the CompletedTransaction becomes `Mined` and the funds
/// that were pending become spent and available respectively.
pub fn mine_transaction<T: WalletBackend>(wallet: &mut Wallet<T>, tx_id: TxId) -> Result<(), WalletError> {
    wallet
        .runtime
        .block_on(wallet.transaction_service.test_mine_transaction(tx_id))?;

    Ok(())
}
