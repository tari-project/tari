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
    support::utils::{make_input, random_string},
    transaction_service::service::setup_transaction_service,
};
use rand::OsRng;
use std::{sync::Mutex, time::Duration};
use tari_comms::peer_manager::{NodeIdentity, PeerFeatures};
use tari_shutdown::Shutdown;
use tari_test_utils::collect_stream;
use tari_transactions::{tari_amount::MicroTari, types::CryptoFactories};
use tari_wallet::transaction_service::{
    callback_handler::CallbackHandler,
    handle::TransactionEvent,
    storage::{
        database::{CompletedTransaction, InboundTransaction, TransactionDatabase},
        memory_db::TransactionMemoryDatabase,
    },
};
use tempdir::TempDir;
use tokio::runtime::Runtime;

#[derive(Debug)]
struct CallbackState {
    pub received_tx_callback_called: bool,
    pub received_tx_reply_callback_called: bool,
    pub received_finalized_tx_callback_called: bool,
    pub broadcast_tx_callback_called: bool,
    pub mined_tx_callback_called: bool,
    pub discovery_send_callback_called: bool,
}

impl CallbackState {
    fn new() -> Self {
        Self {
            received_tx_callback_called: false,
            received_tx_reply_callback_called: false,
            received_finalized_tx_callback_called: false,
            broadcast_tx_callback_called: false,
            mined_tx_callback_called: false,
            discovery_send_callback_called: false,
        }
    }

    fn reset(&mut self) {
        self.received_tx_callback_called = false;
        self.received_tx_reply_callback_called = false;
        self.received_finalized_tx_callback_called = false;
        self.broadcast_tx_callback_called = false;
        self.mined_tx_callback_called = false;
        self.discovery_send_callback_called = false;
    }
}

lazy_static! {
    static ref CALLBACK_STATE: Mutex<CallbackState> = {
        let c = Mutex::new(CallbackState::new());
        c
    };
}

unsafe extern "C" fn received_tx_callback(_tx: *mut InboundTransaction) {
    assert_eq!(_tx.is_null(), false);
    CALLBACK_STATE.lock().unwrap().received_tx_callback_called = true;
    Box::from_raw(_tx);
}

unsafe extern "C" fn received_tx_reply_callback(_tx: *mut CompletedTransaction) {
    assert_eq!(_tx.is_null(), false);
    CALLBACK_STATE.lock().unwrap().received_tx_reply_callback_called = true;

    Box::from_raw(_tx);
}

unsafe extern "C" fn received_finalized_tx_callback(_tx: *mut CompletedTransaction) {
    assert_eq!(_tx.is_null(), false);
    CALLBACK_STATE.lock().unwrap().received_finalized_tx_callback_called = true;

    Box::from_raw(_tx);
}

unsafe extern "C" fn broacast_tx_callback(_tx: *mut CompletedTransaction) {
    assert_eq!(_tx.is_null(), false);
    CALLBACK_STATE.lock().unwrap().broadcast_tx_callback_called = true;

    Box::from_raw(_tx);
}

unsafe extern "C" fn mined_tx_callback(_tx: *mut CompletedTransaction) {
    assert_eq!(_tx.is_null(), false);
    CALLBACK_STATE.lock().unwrap().mined_tx_callback_called = true;

    Box::from_raw(_tx);
}

unsafe extern "C" fn discovery_send_callback(_tx_id: u64, _result: bool) {
    CALLBACK_STATE.lock().unwrap().discovery_send_callback_called = true;
}

#[test]
fn test_callback_handler() {
    let mut runtime = Runtime::new().unwrap();

    let mut rng = OsRng::new().unwrap();
    let factories = CryptoFactories::default();

    CALLBACK_STATE.lock().unwrap().reset();

    // Alice's parameters
    let alice_node_identity = NodeIdentity::random(
        &mut rng,
        "/ip4/127.0.0.1/tcp/33101".parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();

    // Bob's parameters
    let bob_node_identity = NodeIdentity::random(
        &mut rng,
        "/ip4/127.0.0.1/tcp/33102".parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();

    // Carols's parameters
    let carol_node_identity = NodeIdentity::random(
        &mut rng,
        "/ip4/127.0.0.1/tcp/33103".parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();

    let db_tempdir = TempDir::new(random_string(8).as_str()).unwrap();
    let db_folder = db_tempdir.path().to_str().unwrap().to_string();
    let alice_db = TransactionMemoryDatabase::new();
    let (mut alice_ts, mut alice_oms, _alice_comms) = setup_transaction_service(
        &mut runtime,
        alice_node_identity.clone(),
        vec![bob_node_identity.clone()],
        factories.clone(),
        alice_db.clone(),
        db_folder.clone(),
        Duration::from_secs(180),
    );
    let shutdown = Shutdown::new();
    let callback_handler = CallbackHandler::new(
        TransactionDatabase::new(alice_db),
        alice_ts.get_event_stream_fused(),
        shutdown.to_signal(),
        received_tx_callback,
        received_tx_reply_callback,
        received_finalized_tx_callback,
        broacast_tx_callback,
        mined_tx_callback,
        discovery_send_callback,
    );

    runtime.spawn(callback_handler.start());

    let (mut bob_ts, mut bob_oms, _bob_comms) = setup_transaction_service(
        &mut runtime,
        bob_node_identity.clone(),
        vec![alice_node_identity.clone(), carol_node_identity.clone()],
        factories.clone(),
        TransactionMemoryDatabase::new(),
        db_folder.clone(),
        Duration::from_secs(1),
    );

    let (_carol_ts, _carol_oms, _carol_comms) = setup_transaction_service(
        &mut runtime,
        carol_node_identity.clone(),
        vec![bob_node_identity.clone()],
        factories.clone(),
        TransactionMemoryDatabase::new(),
        db_folder.clone(),
        Duration::from_secs(1),
    );

    let value_ab = MicroTari::from(1000);
    let (_utxo, uo1) = make_input(&mut rng, MicroTari(2500), &factories.commitment);

    runtime.block_on(alice_oms.add_output(uo1)).unwrap();

    assert!(runtime
        .block_on(alice_ts.send_transaction(
            carol_node_identity.public_key().clone(),
            value_ab,
            MicroTari::from(20),
            "Yo!".to_string(),
        ))
        .is_err());

    let value_ba = MicroTari::from(750);
    let (_utxo, uo2) = make_input(&mut rng, MicroTari(2500), &factories.commitment);

    runtime.block_on(bob_oms.add_output(uo2)).unwrap();
    runtime
        .block_on(bob_ts.send_transaction(
            alice_node_identity.public_key().clone(),
            value_ba,
            MicroTari::from(20),
            "Hey!".to_string(),
        ))
        .unwrap();

    let alice_event_stream = alice_ts.get_event_stream_fused();

    let alice_stream = collect_stream!(
        runtime,
        alice_event_stream.map(|i| (*i).clone()),
        take = 4,
        timeout = Duration::from_secs(10)
    );

    let recv_tx = alice_stream.iter().find(|i| {
        if let TransactionEvent::ReceivedTransactionReply(_) = i {
            true
        } else {
            false
        }
    });

    if let TransactionEvent::ReceivedTransactionReply(tx_id) = recv_tx.unwrap() {
        runtime.block_on(alice_ts.test_broadcast_transaction(*tx_id)).unwrap();
        runtime.block_on(alice_ts.test_mine_transaction(*tx_id)).unwrap();
    };

    let alice_event_stream = alice_ts.get_event_stream_fused();
    let _ = collect_stream!(
        runtime,
        alice_event_stream.map(|i| (*i).clone()),
        take = 5,
        timeout = Duration::from_secs(10)
    );

    let callback_state = CALLBACK_STATE.lock().unwrap();
    assert!(callback_state.received_tx_callback_called);
    assert!(callback_state.received_tx_reply_callback_called);
    assert!(callback_state.received_finalized_tx_callback_called);
    assert!(callback_state.broadcast_tx_callback_called);
    assert!(callback_state.mined_tx_callback_called);
    assert!(callback_state.discovery_send_callback_called);
}
