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
use crate::support::{
    comms_and_services::setup_comms_services,
    data::{clean_up_datastore, init_datastore},
    utils::assert_change,
};
use futures::executor::ThreadPool;

use rand::{CryptoRng, OsRng, Rng};
use std::{sync::Arc, thread, time::Duration};
use tari_comms::{builder::CommsServices, peer_manager::NodeIdentity};
use tari_core::{
    tari_amount::*,
    transaction::{OutputFeatures, TransactionInput, UnblindedOutput},
    transaction_protocol::recipient::RecipientState,
    types::{PrivateKey, PublicKey, COMMITMENT_FACTORY},
};
use tari_crypto::{
    commitment::HomomorphicCommitmentFactory,
    keys::{PublicKey as PK, SecretKey as SK},
};
use tari_p2p::{
    sync_services::{ServiceExecutor, ServiceRegistry},
    tari_message::TariMessageType,
};
use tari_storage::lmdb_store::LMDBDatabase;
use tari_wallet::{
    output_manager_service::output_manager_service::{OutputManagerService, OutputManagerServiceApi},
    transaction_service::{TransactionService, TransactionServiceApi},
};

pub fn setup_transaction_service(
    seed_key: PrivateKey,
    node_identity: NodeIdentity,
    peers: Vec<NodeIdentity>,
    peer_database: LMDBDatabase,
) -> (
    ServiceExecutor,
    Arc<TransactionServiceApi>,
    Arc<OutputManagerServiceApi>,
    CommsServices<TariMessageType>,
)
{
    let output_manager = OutputManagerService::new(seed_key, "".to_string(), 0);
    let output_manager_api = output_manager.get_api();
    let tx_service = TransactionService::new(output_manager_api.clone());
    let tx_service_api = tx_service.get_api();
    let services = ServiceRegistry::new().register(tx_service).register(output_manager);
    let comms = setup_comms_services(node_identity, peers, peer_database);

    (
        ServiceExecutor::execute(&comms, services),
        tx_service_api,
        output_manager_api,
        comms,
    )
}
pub fn make_input<R: Rng + CryptoRng>(rng: &mut R, val: MicroTari) -> (TransactionInput, UnblindedOutput) {
    let key = PrivateKey::random(rng);
    let commitment = COMMITMENT_FACTORY.commit_value(&key, val.into());
    let input = TransactionInput::new(OutputFeatures::default(), commitment);
    (input, UnblindedOutput::new(val, key, None))
}
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

#[test]
fn manage_single_transaction() {
    let mut rng = OsRng::new().unwrap();
    // Alice's parameters
    let alice_seed = PrivateKey::random(&mut rng);
    let alice_node_identity = NodeIdentity::random(&mut rng, "127.0.0.1:31583".parse().unwrap()).unwrap();
    let alice_database_name = "alice_test_tx_service1"; // Note: every test should have unique database
    let alice_datastore = init_datastore(alice_database_name).unwrap();
    let alice_peer_database = alice_datastore.get_handle(alice_database_name).unwrap();
    // Bob's parameters
    let bob_seed = PrivateKey::random(&mut rng);
    let bob_node_identity = NodeIdentity::random(&mut rng, "127.0.0.1:31582".parse().unwrap()).unwrap();
    let bob_database_name = "bob_test_tx_service1"; // Note: every test should have unique database
    let bob_datastore = init_datastore(bob_database_name).unwrap();
    let bob_peer_database = bob_datastore.get_handle(bob_database_name).unwrap();

    let (alice_services, alice_tx_api, alice_oms_api, mut alice_comms) = setup_transaction_service(
        alice_seed,
        alice_node_identity.clone(),
        vec![bob_node_identity.clone()],
        alice_peer_database,
    );

    let mut thread_pool = ThreadPool::new().unwrap();
    alice_comms.spawn_tasks(&mut thread_pool);

    thread::sleep(Duration::from_millis(500));

    let value = MicroTari::from(1000);
    let (_utxo, uo1) = make_input(&mut rng, MicroTari(2500));

    assert!(alice_tx_api
        .send_transaction(
            bob_node_identity.identity.public_key.clone(),
            value,
            MicroTari::from(20),
        )
        .is_err());

    alice_oms_api.add_output(uo1).unwrap();

    alice_tx_api
        .send_transaction(
            bob_node_identity.identity.public_key.clone(),
            value,
            MicroTari::from(20),
        )
        .unwrap();

    let alice_pending_outbound = alice_tx_api.get_pending_outbound_transaction().unwrap();
    let alice_completed_tx = alice_tx_api.get_completed_transaction().unwrap();
    assert_eq!(alice_pending_outbound.len(), 1);
    assert_eq!(alice_completed_tx.len(), 0);

    let (bob_services, bob_tx_api, bob_oms_api, mut bob_comms) = setup_transaction_service(
        bob_seed,
        bob_node_identity.clone(),
        vec![alice_node_identity.clone()],
        bob_peer_database,
    );
    bob_comms.spawn_tasks(&mut thread_pool);

    assert_change(|| alice_tx_api.get_completed_transaction().unwrap().len(), 1, 50);

    let alice_pending_outbound = alice_tx_api.get_pending_outbound_transaction().unwrap();
    let alice_completed_tx = alice_tx_api.get_completed_transaction().unwrap();
    assert_eq!(alice_pending_outbound.len(), 0);
    assert_eq!(alice_completed_tx.len(), 1);

    let bob_pending_inbound_tx = bob_tx_api.get_pending_inbound_transaction().unwrap();
    assert_eq!(bob_pending_inbound_tx.len(), 1);

    let mut alice_tx_id = 0;
    for (k, _v) in alice_completed_tx.iter() {
        alice_tx_id = k.clone();
    }
    for (k, v) in bob_pending_inbound_tx.iter() {
        assert_eq!(*k, alice_tx_id);
        if let RecipientState::Finalized(rsm) = &v.state {
            bob_oms_api
                .confirm_received_output(alice_tx_id, rsm.output.clone())
                .unwrap();
            assert_eq!(bob_oms_api.get_balance().unwrap(), value);
        } else {
            assert!(false);
        }
    }

    alice_services.shutdown().unwrap();
    bob_services.shutdown().unwrap();
    clean_up_datastore(alice_database_name);
    clean_up_datastore(bob_database_name);
}

#[test]
fn manage_multiple_transactions() {
    let _ = env_logger::builder().is_test(true).try_init();
    let mut rng = OsRng::new().unwrap();
    // Alice's parameters
    let alice_seed = PrivateKey::random(&mut rng);
    let alice_node_identity = NodeIdentity::random(&mut rng, "127.0.0.1:31584".parse().unwrap()).unwrap();
    let alice_database_name = "alice_test_tx_service2"; // Note: every test should have unique database
    let alice_datastore = init_datastore(alice_database_name).unwrap();
    let alice_peer_database = alice_datastore.get_handle(alice_database_name).unwrap();
    // Bob's parameters
    let bob_seed = PrivateKey::random(&mut rng);
    let bob_node_identity = NodeIdentity::random(&mut rng, "127.0.0.1:31585".parse().unwrap()).unwrap();
    let bob_database_name = "bob_test_tx_service2"; // Note: every test should have unique database
    let bob_datastore = init_datastore(bob_database_name).unwrap();
    let bob_peer_database = bob_datastore.get_handle(bob_database_name).unwrap();
    // Carols's parameters
    let carol_seed = PrivateKey::random(&mut rng);
    let carol_node_identity = NodeIdentity::random(&mut rng, "127.0.0.1:31586".parse().unwrap()).unwrap();
    let carol_database_name = "carol_test_tx_service2"; // Note: every test should have unique database
    let carol_datastore = init_datastore(carol_database_name).unwrap();
    let carol_peer_database = carol_datastore.get_handle(carol_database_name).unwrap();
    let (alice_services, alice_tx_api, alice_oms_api, mut alice_comms) = setup_transaction_service(
        alice_seed,
        alice_node_identity.clone(),
        vec![bob_node_identity.clone(), carol_node_identity.clone()],
        alice_peer_database,
    );

    let mut thread_pool = ThreadPool::new().unwrap();
    alice_comms.spawn_tasks(&mut thread_pool);

    // Add some funds to Alices wallet
    let (_utxo, uo1a) = make_input(&mut rng, MicroTari(5500));
    alice_oms_api.add_output(uo1a).unwrap();
    let (_utxo, uo1b) = make_input(&mut rng, MicroTari(3000));
    alice_oms_api.add_output(uo1b).unwrap();
    let (_utxo, uo1c) = make_input(&mut rng, MicroTari(3000));
    alice_oms_api.add_output(uo1c).unwrap();

    // A series of interleaved transactions. First with Bob and Carol offline and then two with them online
    let value_a_to_b_1 = MicroTari::from(1000);
    let value_a_to_b_2 = MicroTari::from(800);
    let value_b_to_a_1 = MicroTari::from(1100);
    let value_a_to_c_1 = MicroTari::from(1400);
    alice_tx_api
        .send_transaction(
            bob_node_identity.identity.public_key.clone(),
            value_a_to_b_1,
            MicroTari::from(20),
        )
        .unwrap();
    alice_tx_api
        .send_transaction(
            carol_node_identity.identity.public_key.clone(),
            value_a_to_c_1,
            MicroTari::from(20),
        )
        .unwrap();
    let alice_pending_outbound = alice_tx_api.get_pending_outbound_transaction().unwrap();
    let alice_completed_tx = alice_tx_api.get_completed_transaction().unwrap();
    assert_eq!(alice_pending_outbound.len(), 2);
    assert_eq!(alice_completed_tx.len(), 0);

    // Spin up Bob and Carol
    let (bob_services, bob_tx_api, bob_oms_api, mut bob_comms) = setup_transaction_service(
        bob_seed,
        bob_node_identity.clone(),
        vec![alice_node_identity.clone()],
        bob_peer_database,
    );
    let (carol_services, carol_tx_api, carol_oms_api, mut carol_comms) = setup_transaction_service(
        carol_seed,
        carol_node_identity.clone(),
        vec![alice_node_identity.clone()],
        carol_peer_database,
    );

    bob_comms.spawn_tasks(&mut thread_pool);
    carol_comms.spawn_tasks(&mut thread_pool);

    let (_utxo, uo2) = make_input(&mut rng, MicroTari(3500));
    bob_oms_api.add_output(uo2).unwrap();
    let (_utxo, uo3) = make_input(&mut rng, MicroTari(4500));
    carol_oms_api.add_output(uo3).unwrap();

    bob_tx_api
        .send_transaction(
            alice_node_identity.identity.public_key.clone(),
            value_b_to_a_1,
            MicroTari::from(20),
        )
        .unwrap();
    alice_tx_api
        .send_transaction(
            bob_node_identity.identity.public_key.clone(),
            value_a_to_b_2,
            MicroTari::from(20),
        )
        .unwrap();

    assert_change(|| alice_tx_api.get_completed_transaction().unwrap().len(), 3, 50);

    let alice_pending_outbound = alice_tx_api.get_pending_outbound_transaction().unwrap();
    let alice_completed_tx = alice_tx_api.get_completed_transaction().unwrap();
    assert_eq!(alice_pending_outbound.len(), 0);
    assert_eq!(alice_completed_tx.len(), 3);
    let bob_pending_outbound = bob_tx_api.get_pending_outbound_transaction().unwrap();
    let bob_completed_tx = bob_tx_api.get_completed_transaction().unwrap();
    assert_eq!(bob_pending_outbound.len(), 0);
    assert_eq!(bob_completed_tx.len(), 1);
    let carol_pending_inbound = carol_tx_api.get_pending_inbound_transaction().unwrap();
    assert_eq!(carol_pending_inbound.len(), 1);

    alice_services.shutdown().unwrap();
    bob_services.shutdown().unwrap();
    carol_services.shutdown().unwrap();
    clean_up_datastore(alice_database_name);
    clean_up_datastore(bob_database_name);
    clean_up_datastore(carol_database_name);
}

// TODO Test the following once the Tokio future based service architecture is in place. The current architecture
// makes it impossible to test this service without a running Service and Comms stack but then you cannot access the
// internals of the service as it is running the ServiceExecutor Thread
//
// What happens when repeated tx_id are sent to be accepted
// What happens with malformed sender message
// What happens with malformed recipient message
// What happens when accepting recipient reply for unknown tx_id
