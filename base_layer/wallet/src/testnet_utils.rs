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
    error::WalletError,
    storage::{database::WalletBackend, memory_db::WalletMemoryDatabase},
    wallet::WalletConfig,
    Wallet,
};
use rand::{distributions::Alphanumeric, CryptoRng, OsRng, Rng, RngCore};
use std::{iter, sync::Arc, thread, time::Duration};
use tari_comms::{
    control_service::ControlServiceConfig,
    peer_manager::{NodeIdentity, PeerFeatures},
    types::{CommsPublicKey, CommsSecretKey},
};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    keys::{PublicKey as PublicKeyTrait, SecretKey as SecretKeyTrait},
};
use tari_p2p::initialization::CommsConfig;
use tari_transactions::{
    tari_amount::MicroTari,
    transaction::{OutputFeatures, TransactionInput, UnblindedOutput},
    types::{PrivateKey, PublicKey, COMMITMENT_FACTORY},
};
use tempdir::TempDir;
use tokio::runtime::Runtime;

// The functions in this module are strictly meant for testing and development of wallet applications without needing to
// spin up other wallets or base nodes before TestNet is live.

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
pub fn make_input<R: Rng + CryptoRng>(rng: &mut R, val: MicroTari) -> (TransactionInput, UnblindedOutput) {
    let key = PrivateKey::random(rng);
    let commitment = COMMITMENT_FACTORY.commit_value(&key, val.into());
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

    let node_id = NodeIdentity::new(
        secret_key,
        net_address.as_str().parse().expect("Invalid Net Address"),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .expect("Could not construct Node Id");

    let comms_config = CommsConfig {
        node_identity: Arc::new(node_id.clone()),
        peer_connection_listening_address: "127.0.0.1".parse().unwrap(),
        socks_proxy_address: None,
        control_service: ControlServiceConfig {
            listener_address: node_id.control_service_address(),
            socks_proxy_address: None,
            requested_connection_timeout: Duration::from_millis(2000),
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
        dht: Default::default(),
    };

    let config = WalletConfig { comms_config };

    Wallet::new(config, WalletMemoryDatabase::new(), runtime).expect("Could not create Wallet")
}

/// This function will generate a set of test data for the supplied wallet. Takes a few seconds to complete
pub fn generate_wallet_test_data<T: WalletBackend>(wallet: &mut Wallet<T>) -> Result<(), WalletError> {
    let mut rng = rand::OsRng::new().unwrap();
    let names = ["Alice", "Bob", "Carol", "Dave"];
    // Generate contacts
    let mut generated_contacts = Vec::new();
    for i in 0..names.len() {
        let (secret_key, public_key): (CommsSecretKey, CommsPublicKey) = PublicKey::random_keypair(&mut rng);
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
    for _i in 0..num_outputs {
        let (_ti, uo) = make_input(
            &mut rng.clone(),
            MicroTari::from(2_000_000 + rng.next_u64() % 1_000_000),
        );
        wallet
            .runtime
            .block_on(wallet.output_manager_service.add_output(uo))
            .unwrap();
    }

    // Generate some Tx history
    let mut wallet_alice = create_wallet(generated_contacts[0].0.clone(), generated_contacts[0].1.clone());
    for _i in 0..20 {
        let (_ti, uo) = make_input(
            &mut rng.clone(),
            MicroTari::from(2_000_000 + rng.next_u64() % 1_000_000),
        );
        wallet_alice
            .runtime
            .block_on(wallet_alice.output_manager_service.add_output(uo))
            .unwrap();
    }

    let mut wallet_bob = create_wallet(generated_contacts[1].0.clone(), generated_contacts[1].1.clone());
    for _i in 0..20 {
        let (_ti, uo) = make_input(
            &mut rng.clone(),
            MicroTari::from(2_000_000 + rng.next_u64() % 1_000_000),
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
        ))
        .expect("Could not send test transaction");

    wallet
        .runtime
        .block_on(wallet.transaction_service.send_transaction(
            contacts[0].public_key.clone(),
            MicroTari::from(20_000),
            MicroTari::from(110),
        ))
        .expect("Could not send test transaction");

    wallet
        .runtime
        .block_on(wallet.transaction_service.send_transaction(
            contacts[1].public_key.clone(),
            MicroTari::from(30_000),
            MicroTari::from(105),
        ))
        .expect("Could not send test transaction");

    // Pending Outbound
    wallet
        .runtime
        .block_on(wallet.transaction_service.send_transaction(
            contacts[2].public_key.clone(),
            MicroTari::from(25_000),
            MicroTari::from(107),
        ))
        .expect("Could not send test transaction");

    wallet
        .runtime
        .block_on(wallet.transaction_service.send_transaction(
            contacts[3].public_key.clone(),
            MicroTari::from(35_000),
            MicroTari::from(117),
        ))
        .expect("Could not send test transaction");

    // Pending Inbound
    wallet_alice
        .runtime
        .block_on(wallet_alice.transaction_service.send_transaction(
            wallet.comms.node_identity().public_key().clone(),
            MicroTari::from(35_000),
            MicroTari::from(117),
        ))
        .expect("Could not send test transaction");
    wallet_bob
        .runtime
        .block_on(wallet_bob.transaction_service.send_transaction(
            wallet.comms.node_identity().public_key().clone(),
            MicroTari::from(35_000),
            MicroTari::from(117),
        ))
        .expect("Could not send test transaction");

    thread::sleep(Duration::from_millis(1000));

    Ok(())
}
