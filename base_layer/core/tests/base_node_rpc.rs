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
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

mod helpers;

use crate::helpers::{
    block_builders::{chain_block, create_genesis_block_with_coinbase_value},
    nodes::{BaseNodeBuilder, NodeInterfaces},
};
use std::convert::TryFrom;
use tari_comms::protocol::rpc::mock::RpcRequestMock;
use tari_core::{
    base_node::{
        comms_interface::Broadcast,
        proto::wallet_response::{TxLocation, TxQueryResponse, TxSubmissionRejectionReason, TxSubmissionResponse},
        rpc::{BaseNodeWalletRpcService, BaseNodeWalletService},
        state_machine_service::states::{ListeningInfo, StateInfo, StatusInfo},
    },
    chain_storage::ChainBlock,
    consensus::{ConsensusManager, ConsensusManagerBuilder, Network},
    crypto::tari_utilities::Hashable,
    proto::generated::types::{Signature as SignatureProto, Transaction as TransactionProto},
    test_helpers::blockchain::TempDatabase,
    transactions::{
        helpers::schema_to_transaction,
        tari_amount::{uT, T},
        transaction::UnblindedOutput,
        types::CryptoFactories,
    },
    txn_schema,
};
use tempfile::{tempdir, TempDir};
use tokio::runtime::Runtime;

fn setup() -> (
    BaseNodeWalletRpcService<TempDatabase>,
    NodeInterfaces,
    RpcRequestMock,
    ConsensusManager,
    ChainBlock,
    UnblindedOutput,
    Runtime,
    TempDir,
) {
    let network = Network::LocalNet;
    let consensus_constants = network.create_consensus_constants();
    let factories = CryptoFactories::default();
    let mut runtime = Runtime::new().unwrap();
    let temp_dir = tempdir().unwrap();

    let (block0, utxo0) =
        create_genesis_block_with_coinbase_value(&factories, 100_000_000.into(), &consensus_constants[0]);
    let consensus_manager = ConsensusManagerBuilder::new(network)
        .with_consensus_constants(consensus_constants[0].clone())
        .with_block(block0.clone())
        .build();

    let (mut base_node, _consensus_manager) = BaseNodeBuilder::new(network)
        .with_consensus_manager(consensus_manager.clone())
        .start(&mut runtime, temp_dir.path().to_str().unwrap());
    base_node.mock_base_node_state_machine.publish_status(StatusInfo {
        bootstrapped: true,
        state_info: StateInfo::Listening(ListeningInfo::new(true)),
    });

    let request_mock = runtime.enter(|| RpcRequestMock::new(base_node.comms.peer_manager()));
    let service =
        BaseNodeWalletRpcService::new(base_node.blockchain_db.clone().into(), base_node.mempool_handle.clone());
    (
        service,
        base_node,
        request_mock,
        consensus_manager,
        block0,
        utxo0,
        runtime,
        temp_dir,
    )
}

#[test]
fn test_base_node_wallet_rpc() {
    // Testing the submit_transaction() and transaction_query() rpc calls
    let (service, mut base_node, request_mock, consensus_manager, block0, utxo0, mut runtime, _temp_dir) = setup();

    let (txs1, utxos1) = schema_to_transaction(&vec![txn_schema!(from: vec![utxo0.clone()], to: vec![1 * T, 1 * T])]);
    let tx1 = (*txs1[0]).clone();
    let tx1_sig = tx1.first_kernel_excess_sig().clone().unwrap().clone();

    let (txs2, _utxos2) = schema_to_transaction(&vec![txn_schema!(
        from: vec![utxos1[0].clone()],
        to: vec![400_000 * uT, 590_000 * uT]
    )]);
    let tx2 = (*txs2[0]).clone();
    let tx2_sig = tx2.first_kernel_excess_sig().clone().unwrap().clone();

    // Query Tx1
    let msg = SignatureProto::from(tx1_sig.clone());
    let req = request_mock.request_with_context(Default::default(), msg.clone());
    let resp =
        TxQueryResponse::try_from(runtime.block_on(service.transaction_query(req)).unwrap().into_message()).unwrap();

    assert_eq!(resp.confirmations, 0);
    assert_eq!(resp.block_hash, None);
    assert_eq!(resp.location, TxLocation::NotStored);

    // First lets try submit tx2 which will be an orphan tx
    let msg = TransactionProto::from(tx2.clone());
    let req = request_mock.request_with_context(Default::default(), msg.clone());

    let resp = TxSubmissionResponse::try_from(
        runtime
            .block_on(service.submit_transaction(req))
            .unwrap()
            .into_message(),
    )
    .unwrap();

    assert!(!resp.accepted);
    assert_eq!(resp.rejection_reason, TxSubmissionRejectionReason::Orphan);

    // Query Tx2 to confirm it wasn't accepted
    let msg = SignatureProto::from(tx2_sig.clone());
    let req = request_mock.request_with_context(Default::default(), msg.clone());
    let resp =
        TxQueryResponse::try_from(runtime.block_on(service.transaction_query(req)).unwrap().into_message()).unwrap();

    assert_eq!(resp.confirmations, 0);
    assert_eq!(resp.block_hash, None);
    assert_eq!(resp.location, TxLocation::NotStored);

    // Now submit a block with Tx1 in it so that Tx2 is no longer an orphan
    let block1 = base_node
        .blockchain_db
        .prepare_block_merkle_roots(chain_block(&block0.block, vec![tx1.clone()], &consensus_manager))
        .unwrap();

    assert!(runtime
        .block_on(base_node.local_nci.submit_block(block1.clone(), Broadcast::from(true)))
        .is_ok());

    // Check that subitting Tx2 will now be accepted
    let msg = TransactionProto::from(tx2.clone());
    let req = request_mock.request_with_context(Default::default(), msg);
    let resp = runtime
        .block_on(service.submit_transaction(req))
        .unwrap()
        .into_message();

    assert!(resp.accepted);

    // Query Tx2 which should now be in the mempool
    let msg = SignatureProto::from(tx2_sig.clone());
    let req = request_mock.request_with_context(Default::default(), msg.clone());
    let resp =
        TxQueryResponse::try_from(runtime.block_on(service.transaction_query(req)).unwrap().into_message()).unwrap();

    assert_eq!(resp.confirmations, 0);
    assert_eq!(resp.block_hash, None);
    assert_eq!(resp.location, TxLocation::InMempool);

    // Now if we submit Tx1 is should return as rejected as AlreadyMined as Tx1's kernel is present
    let msg = TransactionProto::from(tx1.clone());
    let req = request_mock.request_with_context(Default::default(), msg);
    let resp = TxSubmissionResponse::try_from(
        runtime
            .block_on(service.submit_transaction(req))
            .unwrap()
            .into_message(),
    )
    .unwrap();

    assert!(!resp.accepted);
    assert_eq!(resp.rejection_reason, TxSubmissionRejectionReason::AlreadyMined);

    // Now create a different tx that uses the same input as Tx1 to produce a DoubleSpend rejection
    let (txs1b, _utxos1) = schema_to_transaction(&vec![txn_schema!(from: vec![utxo0.clone()], to: vec![2 * T, 1 * T])]);
    let tx1b = (*txs1b[0]).clone();

    // Now if we submit Tx1 is should return as rejected as AlreadyMined
    let msg = TransactionProto::from(tx1b.clone());
    let req = request_mock.request_with_context(Default::default(), msg);
    let resp = TxSubmissionResponse::try_from(
        runtime
            .block_on(service.submit_transaction(req))
            .unwrap()
            .into_message(),
    )
    .unwrap();

    assert!(!resp.accepted);
    assert_eq!(resp.rejection_reason, TxSubmissionRejectionReason::DoubleSpend);

    // Now we will Mine block 2 so that we can see 1 confirmation on tx1
    let mut block2 = base_node
        .blockchain_db
        .prepare_block_merkle_roots(chain_block(&block1, vec![], &consensus_manager))
        .unwrap();

    block2.header.output_mmr_size += 1;
    block2.header.kernel_mmr_size += 1;

    runtime
        .block_on(base_node.local_nci.submit_block(block2.clone(), Broadcast::from(true)))
        .unwrap();

    // Query Tx1 which should be in block 1 with 1 confirmation
    let msg = SignatureProto::from(tx1_sig.clone());
    let req = request_mock.request_with_context(Default::default(), msg.clone());
    let resp =
        TxQueryResponse::try_from(runtime.block_on(service.transaction_query(req)).unwrap().into_message()).unwrap();

    assert_eq!(resp.confirmations, 1);
    assert_eq!(resp.block_hash, Some(block1.hash()));
    assert_eq!(resp.location, TxLocation::Mined);
}
