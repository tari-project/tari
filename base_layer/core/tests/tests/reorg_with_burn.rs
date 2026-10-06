// Copyright 2024. The Tari Project
//
// Reorg-driven integration tests for the output-SMT undo path. Unlike a plain `rewind_to_height`,
// these go through the real `swap_to_highest_pow_chain` -> `reorganize_chain` flow, which rewinds
// the losing chain (here: blocks containing burns + cross-block spends) and applies a competing
// fork. The second test forces a fork block to fail mid-reorg so the `restore_reorged_chain` path
// re-applies the original chain. Both assert the SMT round-trips without the
// "Deleting block, new smt root did not match expected smt root" error.
#![allow(clippy::indexing_slicing)]
// Overflow in test code panics, which is the desired failure mode for a test.
#![allow(clippy::arithmetic_side_effects)]
use std::sync::{Arc, Mutex};

use tari_common::configuration::Network;
use tari_common_types::{chain_metadata::ChainMetadata, types::FixedHash};
use tari_core::{
    chain_storage::{BlockAddResult, BlockchainDatabase, BlockchainDatabaseConfig, Validators},
    consensus::BaseNodeConsensusManager,
    test_helpers::blockchain::{TempDatabase, create_test_db},
    validation::{CandidateBlockValidator, DifficultyCalculator, ValidationError, mocks::MockValidator},
};
use tari_node_components::blocks::ChainBlock;
use tari_transaction_components::{
    consensus::ConsensusConstantsBuilder,
    key_manager::KeyManager,
    tari_amount::{T, uT},
    tari_proof_of_work::Difficulty,
    transaction_components::OutputFeatures,
    txn_schema,
};

use crate::helpers::block_builders::{
    create_genesis_block_with_utxos,
    generate_new_block,
    generate_new_block_with_achieved_difficulty,
};

fn burn_schema(
    input: tari_transaction_components::transaction_components::WalletOutput,
    amount: u64,
) -> tari_transaction_components::test_helpers::TransactionSchema {
    txn_schema!(
        from: vec![input],
        to: vec![amount * uT],
        fee: 5.into(),
        lock: 0,
        features: OutputFeatures::create_burn_output()
    )
}

fn mock_validators() -> Validators<TempDatabase> {
    Validators::new(
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
    )
}

fn new_store(
    rules: &BaseNodeConsensusManager,
    validators: Validators<TempDatabase>,
) -> BlockchainDatabase<TempDatabase> {
    BlockchainDatabase::start_new(
        create_test_db(),
        rules.clone(),
        validators,
        BlockchainDatabaseConfig::default(),
        DifficultyCalculator::new(rules.clone(), Default::default()),
    )
    .unwrap()
}

#[test]
#[allow(clippy::too_many_lines)]
#[allow(clippy::identity_op)]
fn reorg_rewinds_burn_and_spend_blocks() {
    let key_manager = KeyManager::new_random().unwrap();
    let network = Network::LocalNet;
    let consensus_constants = ConsensusConstantsBuilder::new(network).build();
    let (genesis, gen_outputs) =
        create_genesis_block_with_utxos(&[T, T, T, T, T, T], &consensus_constants, &key_manager);
    let rules = BaseNodeConsensusManager::builder(network)
        .add_consensus_constants(consensus_constants)
        .with_block(genesis.clone())
        .build()
        .unwrap();

    let mut store = new_store(&rules, mock_validators());
    let mut blocks = vec![genesis.clone()];
    let mut outputs = vec![gen_outputs.clone()];

    // Main chain: GB -> A1 -> A2(burn) -> A3(burn), with cross-block spends.
    let s = vec![txn_schema!(from: vec![outputs[0][0].clone()], to: vec![10 * T, 10 * T, 10 * T])];
    generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        s,
        Difficulty::from_u64(1).unwrap(),
        &rules,
        &key_manager,
    )
    .unwrap();
    let s = vec![
        txn_schema!(from: vec![outputs[1][0].clone()], to: vec![5 * T]),
        burn_schema(outputs[0][1].clone(), 700_000),
    ];
    generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        s,
        Difficulty::from_u64(2).unwrap(),
        &rules,
        &key_manager,
    )
    .unwrap();
    let s = vec![
        txn_schema!(from: vec![outputs[2][0].clone()], to: vec![2 * T]),
        burn_schema(outputs[0][2].clone(), 800_000),
    ];
    generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        s,
        Difficulty::from_u64(2).unwrap(),
        &rules,
        &key_manager,
    )
    .unwrap();
    assert_eq!(store.get_height().unwrap(), 3);

    // Build a LONGER competing fork off A1 on a separate store.
    let mut orphan_store = new_store(&rules, mock_validators());
    orphan_store.add_block(blocks[1].to_arc_block()).unwrap();
    let mut ob = vec![blocks[0].clone(), blocks[1].clone()];
    let mut oo = vec![outputs[0].clone(), outputs[1].clone()];
    let s = vec![
        txn_schema!(from: vec![oo[1][0].clone()], to: vec![6 * T]),
        burn_schema(oo[1][1].clone(), 500_000),
    ];
    generate_new_block_with_achieved_difficulty(
        &mut orphan_store,
        &mut ob,
        &mut oo,
        s,
        Difficulty::from_u64(3).unwrap(),
        &rules,
        &key_manager,
    )
    .unwrap();
    let s = vec![txn_schema!(from: vec![oo[2][0].clone()], to: vec![3 * T])];
    generate_new_block_with_achieved_difficulty(
        &mut orphan_store,
        &mut ob,
        &mut oo,
        s,
        Difficulty::from_u64(3).unwrap(),
        &rules,
        &key_manager,
    )
    .unwrap();
    let s = vec![
        txn_schema!(from: vec![oo[3][0].clone()], to: vec![1 * T]),
        burn_schema(oo[1][2].clone(), 400_000),
    ];
    generate_new_block_with_achieved_difficulty(
        &mut orphan_store,
        &mut ob,
        &mut oo,
        s,
        Difficulty::from_u64(3).unwrap(),
        &rules,
        &key_manager,
    )
    .unwrap();
    let s = vec![txn_schema!(from: vec![oo[4][0].clone()], to: vec![500_000 * uT])];
    generate_new_block_with_achieved_difficulty(
        &mut orphan_store,
        &mut ob,
        &mut oo,
        s,
        Difficulty::from_u64(3).unwrap(),
        &rules,
        &key_manager,
    )
    .unwrap();

    // ob = [GB, A1, B2, B3, B4, B5]. Feed the fork blocks to the main store. Once the fork outweighs
    // the main chain, `add_block` reorgs: it rewinds A2/A3 (burn + spend blocks) via `delete_tip_block`
    // and applies the fork. A reorg that hit the SMT bug would surface here as an Err.
    let mut reorg_removed = 0usize;
    for b in &ob[2..] {
        match store.add_block(b.to_arc_block()).unwrap() {
            BlockAddResult::ChainReorg { removed, .. } => reorg_removed = removed.len(),
            other => {
                assert!(
                    matches!(other, BlockAddResult::Ok(_) | BlockAddResult::OrphanBlock),
                    "unexpected add result for fork block {}",
                    b.height()
                );
            },
        }
    }

    // A reorg must have occurred and rewound the two losing burn/spend blocks (A2, A3).
    assert_eq!(reorg_removed, 2, "expected the reorg to rewind both A2 and A3");
    // The tip is now the fork tip, and the SMT agreed with every rewound + re-applied header.
    assert_eq!(store.fetch_tip_header().unwrap().header(), ob.last().unwrap().header());
    assert_eq!(store.get_height().unwrap(), 5);
}

#[test]
#[allow(clippy::too_many_lines)]
#[allow(clippy::identity_op)]
fn failed_reorg_restores_burn_and_spend_chain() {
    let key_manager = KeyManager::new_random().unwrap();
    let network = Network::LocalNet;
    let consensus_constants = ConsensusConstantsBuilder::new(network).build();
    let (genesis, gen_outputs) =
        create_genesis_block_with_utxos(&[T, T, T, T, T, T], &consensus_constants, &key_manager);
    let rules = BaseNodeConsensusManager::builder(network)
        .add_consensus_constants(consensus_constants)
        .with_block(genesis.clone())
        .build()
        .unwrap();

    // Main store: the `block` validator fails only for a hash we set later (a fork block).
    let fail = FailOnHash {
        bad: Arc::new(Mutex::new(None)),
    };
    let mut store = new_store(
        &rules,
        Validators::new(fail.clone(), MockValidator::new(true), MockValidator::new(true)),
    );
    let mut blocks = vec![genesis.clone()];
    let mut outputs = vec![gen_outputs.clone()];

    // Original main chain GB -> A1 -> A2(burn) -> A3(burn) (bad == None, so all validate).
    let s = vec![txn_schema!(from: vec![outputs[0][0].clone()], to: vec![10 * T, 10 * T, 10 * T])];
    generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        s,
        Difficulty::from_u64(1).unwrap(),
        &rules,
        &key_manager,
    )
    .unwrap();
    let s = vec![
        txn_schema!(from: vec![outputs[1][0].clone()], to: vec![5 * T]),
        burn_schema(outputs[0][1].clone(), 700_000),
    ];
    generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        s,
        Difficulty::from_u64(2).unwrap(),
        &rules,
        &key_manager,
    )
    .unwrap();
    let s = vec![
        txn_schema!(from: vec![outputs[2][0].clone()], to: vec![2 * T]),
        burn_schema(outputs[0][2].clone(), 800_000),
    ];
    generate_new_block_with_achieved_difficulty(
        &mut store,
        &mut blocks,
        &mut outputs,
        s,
        Difficulty::from_u64(2).unwrap(),
        &rules,
        &key_manager,
    )
    .unwrap();
    let original_tip = store.fetch_tip_header().unwrap();
    let original_output_mr = original_tip.header().output_mr;

    // Build a heavier fork off A1 on a separate store.
    let mut orphan_store = new_store(&rules, mock_validators());
    orphan_store.add_block(blocks[1].to_arc_block()).unwrap();
    let mut ob = vec![blocks[0].clone(), blocks[1].clone()];
    let mut oo = vec![outputs[0].clone(), outputs[1].clone()];
    let s = vec![
        txn_schema!(from: vec![oo[1][0].clone()], to: vec![6 * T]),
        burn_schema(oo[1][1].clone(), 500_000),
    ];
    generate_new_block_with_achieved_difficulty(
        &mut orphan_store,
        &mut ob,
        &mut oo,
        s,
        Difficulty::from_u64(3).unwrap(),
        &rules,
        &key_manager,
    )
    .unwrap();
    let s = vec![txn_schema!(from: vec![oo[2][0].clone()], to: vec![3 * T])];
    generate_new_block_with_achieved_difficulty(
        &mut orphan_store,
        &mut ob,
        &mut oo,
        s,
        Difficulty::from_u64(3).unwrap(),
        &rules,
        &key_manager,
    )
    .unwrap();
    let s = vec![txn_schema!(from: vec![oo[3][0].clone()], to: vec![1 * T])];
    generate_new_block_with_achieved_difficulty(
        &mut orphan_store,
        &mut ob,
        &mut oo,
        s,
        Difficulty::from_u64(3).unwrap(),
        &rules,
        &key_manager,
    )
    .unwrap();

    // Make the SECOND fork block (B3) fail when the reorg tries to apply it. The reorg will first
    // rewind A2/A3, apply B2, then fail on B3 -> restore_reorged_chain must put A2/A3 back.
    *fail.bad.lock().unwrap() = Some(*ob[3].hash());

    let mut saw_err = false;
    for b in &ob[2..] {
        if store.add_block(b.to_arc_block()).is_err() {
            saw_err = true;
        }
    }
    assert!(saw_err, "expected the reorg to fail when applying the bad fork block");

    // The original burn/spend chain must be fully restored, SMT included.
    let restored_tip = store.fetch_tip_header().unwrap();
    assert_eq!(
        restored_tip.header(),
        original_tip.header(),
        "original chain tip was not restored"
    );
    assert_eq!(restored_tip.header().output_mr, original_output_mr);
    assert_eq!(store.get_height().unwrap(), 3);

    // And the restored SMT must still unwind cleanly all the way back to genesis.
    for target in (0..3).rev() {
        store
            .rewind_to_height(target)
            .unwrap_or_else(|e| panic!("post-restore rewind to {target} failed: {e}"));
    }
}

/// A `block` validator that fails for one specific block hash (set after the fork is built), used to
/// force a mid-reorg failure and exercise `restore_reorged_chain`.
#[derive(Clone)]
struct FailOnHash {
    bad: Arc<Mutex<Option<FixedHash>>>,
}

impl<B: tari_core::chain_storage::BlockchainBackend> CandidateBlockValidator<B> for FailOnHash {
    fn validate_body_with_metadata(&self, _: &B, block: &ChainBlock, _: &ChainMetadata) -> Result<(), ValidationError> {
        if Some(*block.hash()) == *self.bad.lock().unwrap() {
            return Err(ValidationError::ConsensusError(
                "forced failure for restore test".into(),
            ));
        }
        Ok(())
    }

    fn validate_body_at_height(&self, _: &B, _: &ChainBlock) -> Result<(), ValidationError> {
        Ok(())
    }
}

#[test]
#[allow(clippy::identity_op)]
fn rewind_chain_with_burned_outputs_and_spends_succeeds() {
    let key_manager = KeyManager::new_random().unwrap();
    let network = Network::LocalNet;
    let consensus_constants = ConsensusConstantsBuilder::new(network).build();

    // Genesis with five spendable UTXOs of 1 T each at `outputs[0][0..5]`.
    let (genesis, gen_outputs) = create_genesis_block_with_utxos(&[T, T, T, T, T], &consensus_constants, &key_manager);
    let rules = BaseNodeConsensusManager::builder(network)
        .add_consensus_constants(consensus_constants)
        .with_block(genesis.clone())
        .build()
        .unwrap();

    // Mock validators so we can include burn outputs without constructing full burn kernels; the
    // production SMT insert/undo and merkle-root checks still run unconditionally inside the backend.
    let validators = Validators::new(
        MockValidator::new(true),
        MockValidator::new(true),
        MockValidator::new(true),
    );
    let mut db = BlockchainDatabase::start_new(
        create_test_db(),
        rules.clone(),
        validators,
        BlockchainDatabaseConfig::default(),
        DifficultyCalculator::new(rules.clone(), Default::default()),
    )
    .unwrap();

    let genesis_output_mr = db.fetch_header(0).unwrap().unwrap().output_mr;

    let mut blocks = vec![genesis];
    let mut outputs = vec![gen_outputs];

    // Block 1: a normal spend (two outputs we will spend later) + a BURNED output.
    let schema = vec![
        txn_schema!(from: vec![outputs[0][0].clone()], to: vec![600_000 * uT, 300_000 * uT]),
        txn_schema!(
            from: vec![outputs[0][1].clone()],
            to: vec![700_000 * uT],
            fee: 5.into(),
            lock: 0,
            features: OutputFeatures::create_burn_output()
        ),
    ];
    generate_new_block(&mut db, &mut blocks, &mut outputs, schema, &rules, &key_manager).unwrap();

    // Block 2: spend an output created in block 1 (restored to the SMT on rewind) + another BURN.
    let schema = vec![
        txn_schema!(from: vec![outputs[1][0].clone()], to: vec![400_000 * uT]),
        txn_schema!(
            from: vec![outputs[0][2].clone()],
            to: vec![800_000 * uT],
            fee: 5.into(),
            lock: 0,
            features: OutputFeatures::create_burn_output()
        ),
    ];
    generate_new_block(&mut db, &mut blocks, &mut outputs, schema, &rules, &key_manager).unwrap();

    // Block 3: spend an output created in block 2.
    let schema = vec![txn_schema!(from: vec![outputs[2][0].clone()], to: vec![200_000 * uT])];
    generate_new_block(&mut db, &mut blocks, &mut outputs, schema, &rules, &key_manager).unwrap();

    assert_eq!(db.get_height().unwrap(), 3);

    // Rewind one block at a time — this is the path that emits the failing error in production.
    for target in (0..3).rev() {
        db.rewind_to_height(target).unwrap_or_else(|e| {
            panic!("rewind to height {target} failed (SMT undo): {e}");
        });
        assert_eq!(db.get_height().unwrap(), target);
    }

    // The SMT must be byte-identical to genesis after fully unwinding every block.
    let restored_output_mr = db.fetch_header(0).unwrap().unwrap().output_mr;
    assert_eq!(
        restored_output_mr, genesis_output_mr,
        "output merkle root was not restored to genesis after rewinding burn/spend blocks"
    );
}
