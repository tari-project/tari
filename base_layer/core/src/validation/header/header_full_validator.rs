// Copyright 2022. The Tari Project
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

use std::cmp;

use log::warn;
use tari_common_types::types::FixedHash;
use tari_node_components::blocks::{BlockHeader, BlockHeaderValidationError};
use tari_transaction_components::{
    consensus::ConsensusConstants,
    tari_proof_of_work::{PowAlgorithm, PowError, ProofOfWork},
};
use tari_utilities::{epoch_time::EpochTime, hex::Hex};

use crate::{
    chain_storage::BlockchainBackend,
    consensus::BaseNodeConsensusManager,
    proof_of_work::{AdjustedTarget, cuckaroo_pow::check_cuckaroo_pow_data, monero_rx::MoneroPowData},
    validation::{
        DifficultyCalculator,
        HeaderChainContext,
        HeaderChainLinkedValidator,
        ValidatedHeader,
        ValidationError,
        helpers::{check_header_timestamp_greater_than_median, check_target_difficulty, pow_rejection_message},
    },
};
pub const LOG_TARGET: &str = "c::val::header_full_validator";

#[derive(Clone)]
pub struct HeaderFullValidator {
    rules: BaseNodeConsensusManager,
    difficulty_calculator: DifficultyCalculator,
    gen_hash: FixedHash,
}

impl HeaderFullValidator {
    pub fn new(rules: BaseNodeConsensusManager, difficulty_calculator: DifficultyCalculator) -> Self {
        let gen_hash = *rules.get_genesis_block().hash();
        Self {
            rules,
            difficulty_calculator,
            gen_hash,
        }
    }
}

impl<B: BlockchainBackend> HeaderChainLinkedValidator<B> for HeaderFullValidator {
    fn validate(
        &self,
        db: &B,
        header: &BlockHeader,
        prev_header: &BlockHeader,
        prev_timestamps: &[EpochTime],
        target_difficulty: Option<AdjustedTarget>,
        chain_context: HeaderChainContext<'_>,
    ) -> Result<ValidatedHeader, ValidationError> {
        let constants = self.rules.consensus_constants(header.height);

        check_not_bad_block(db, header.hash())?;
        check_blockchain_version(constants, header.version)?;
        check_height(header, prev_header)?;
        check_prev_hash(header, prev_header)?;

        sanity_check_timestamp_count(header, prev_timestamps, constants)?;
        check_header_timestamp_greater_than_median(header, prev_timestamps)?;

        check_timestamp_ftl(header, &self.rules)?;
        check_pow_data(header, constants).inspect_err(
            |e| warn!(target: LOG_TARGET, "{}", pow_rejection_message(&header.hash(), header, false, e)),
        )?;
        let achieved_target = if let Some(target) = target_difficulty {
            check_target_difficulty(
                header,
                target,
                &self.difficulty_calculator.randomx_factory,
                &self.gen_hash,
                &self.rules,
                chain_context.vm_key(),
            )?
        } else {
            self.difficulty_calculator
                .check_achieved_and_target_difficulty_with_vm_key(db, header, chain_context.vm_key())?
        };

        // After the proof of work, so that a header without it is not recorded as a bad block. The header commits to
        // `prev_hash`, so this verdict cannot change and the header can be blacklisted on it.
        check_kernel_mmr_size(header, prev_header)?;

        // Deliberately last, and deliberately after the proof of work has been checked: the seed age is a chain
        // dependent verdict that a caller must not record against the header itself (see
        // `BlockHeaderSyncValidator::blacklist_unless_verdict_can_change`). A header that fails an absolute check
        // has to fail on that, so that it can still be blacklisted; only a header that is sound on its own terms
        // gets to be judged against the chain it claims to extend. The kernel MMR size check above reads the
        // previous header too, but its verdict is fixed by the header's own `prev_hash`, so it counts as absolute.
        // Height and previous hash are chain dependent; header sync checks them against the previous header before
        // calling this validator.
        let monero_seed = check_monero_seed_height(db, header, constants, &self.rules, chain_context)?;

        Ok(ValidatedHeader {
            achieved_target,
            monero_seed,
        })
    }
}

/// This is a sanity check for the information provided by the caller, rather than a validation for the header itself.
fn sanity_check_timestamp_count(
    header: &BlockHeader,
    timestamps: &[EpochTime],
    consensus_constants: &ConsensusConstants,
) -> Result<(), ValidationError> {
    let expected_timestamp_count = cmp::min(consensus_constants.median_timestamp_count() as u64, header.height);
    // Empty `timestamps` is never valid
    if timestamps.is_empty() {
        return Err(ValidationError::IncorrectNumberOfTimestampsProvided {
            expected: expected_timestamp_count,
            actual: 0,
        });
    }

    if timestamps.len() as u64 != expected_timestamp_count {
        return Err(ValidationError::IncorrectNumberOfTimestampsProvided {
            actual: timestamps.len() as u64,
            expected: expected_timestamp_count,
        });
    }

    Ok(())
}

fn check_height(header: &BlockHeader, prev_header: &BlockHeader) -> Result<(), ValidationError> {
    if header.height != prev_header.height.saturating_add(1) {
        return Err(ValidationError::BlockHeaderError(
            BlockHeaderValidationError::InvalidHeight {
                expected: prev_header.height.saturating_add(1),
                actual: header.height,
            },
        ));
    }

    Ok(())
}

fn check_prev_hash(header: &BlockHeader, prev_header: &BlockHeader) -> Result<(), ValidationError> {
    if header.prev_hash != prev_header.hash() {
        return Err(ValidationError::BlockHeaderError(
            BlockHeaderValidationError::InvalidPreviousHash {
                expected: prev_header.hash(),
                actual: header.prev_hash,
            },
        ));
    }

    Ok(())
}

/// Every block has exactly one coinbase kernel, so the kernel MMR must grow by at least one per block.
fn check_kernel_mmr_size(header: &BlockHeader, prev_header: &BlockHeader) -> Result<(), ValidationError> {
    if header.kernel_mmr_size <= prev_header.kernel_mmr_size {
        return Err(ValidationError::BlockHeaderError(
            BlockHeaderValidationError::KernelMmrSizeNotIncreasing {
                prev: prev_header.kernel_mmr_size,
                actual: header.kernel_mmr_size,
            },
        ));
    }

    Ok(())
}

fn check_blockchain_version(constants: &ConsensusConstants, version: u16) -> Result<(), ValidationError> {
    if constants.valid_blockchain_version_range().contains(&version) {
        Ok(())
    } else {
        Err(ValidationError::InvalidBlockchainVersion { version })
    }
}

/// This function tests that the block timestamp is less than the FTL
pub fn check_timestamp_ftl(
    block_header: &BlockHeader,
    consensus_manager: &BaseNodeConsensusManager,
) -> Result<(), ValidationError> {
    if block_header.timestamp > consensus_manager.consensus_constants(block_header.height).ftl() {
        warn!(
            target: LOG_TARGET,
            "Invalid Future Time Limit on block:{}",
            block_header.hash().to_hex()
        );
        return Err(ValidationError::BlockHeaderError(
            BlockHeaderValidationError::InvalidTimestampFutureTimeLimit,
        ));
    }
    Ok(())
}

fn check_not_bad_block<B: BlockchainBackend>(db: &B, hash: FixedHash) -> Result<(), ValidationError> {
    let (is_bad_block, reason) = db.bad_block_exists(hash)?;
    if is_bad_block {
        return Err(ValidationError::BadBlockFound {
            hash: hash.to_hex(),
            reason,
        });
    }
    Ok(())
}

fn check_allowed_algos(pow: &ProofOfWork, allowed_algos: &[PowAlgorithm]) -> Result<(), ValidationError> {
    if !allowed_algos.contains(&pow.pow_algo) {
        return Err(ValidationError::BlockHeaderError(
            BlockHeaderValidationError::InvalidPowAlgorithm(pow.pow_algo.to_string()),
        ));
    }
    Ok(())
}

/// Check the PoW data in the BlockHeader. This currently only applies to blocks merged mined with Monero.
fn check_pow_data(block_header: &BlockHeader, consensus_constants: &ConsensusConstants) -> Result<(), ValidationError> {
    let allowed_algos = consensus_constants.current_permitted_pow_algos();
    check_allowed_algos(&block_header.pow, &allowed_algos)?;
    check_pow_data_inner(
        &block_header.pow,
        block_header.nonce,
        consensus_constants.cuckaroo_cycle_length(),
        consensus_constants.cuckaroo_edge_bits(),
        consensus_constants.require_canonical_randomxt_pow_data(),
    )
}

/// Enforce the RandomX seed age rule at header validation time.
///
/// The same rule is applied again during block body validation (defence in depth, and for the block sync path), but
/// checking it here means a header chain that can never produce valid bodies is rejected before it is committed.
///
/// The RandomX key (seed) is recoverable from the header alone, so the only thing needed is the height at which the
/// chain this header extends first used that key. Where that comes from is the caller's business - see
/// [`HeaderChainContext::monero_seed_first_seen_height`] - because the database index is keyed by seed alone with no
/// record of which chain used it, and header validation does not always run against a database holding the chain
/// under validation.
///
/// What the rule can see, and therefore where it bites:
///
/// * The header being validated is never itself in the index: nothing inserts a header before validating it. The first
///   use of a seed therefore reads "not seen" and always passes; the rule only bites on re-use.
/// * On the `add_block` path blocks are committed one at a time, so every earlier use of a seed on this chain is in the
///   index and the rule is exact.
/// * Header sync validates a whole candidate chain before committing any of it, and commits in batches after that, so
///   earlier uses on the candidate chain are not in the index when they are needed. The caller supplies them through
///   the chain context instead: `BlockHeaderSyncValidator` records every header it proves, which makes the rule exact
///   there too. A caller that records nothing still gets the rule applied against committed headers, just with as much
///   slack as its commit batching.
///
/// A `pow_data` blob that cannot be parsed is a bannable `ValidationError::MergeMineError` rather than a storage
/// error, but by the time this runs the difficulty check has already parsed the same blob (`verify_header` does it on
/// both the supplied-target and the calculated-target branch), so a malformed blob has failed there and never reaches
/// the parse below. The parse is kept because this function cannot assume its caller ran that check first.
fn check_monero_seed_height<B: BlockchainBackend>(
    db: &B,
    header: &BlockHeader,
    consensus_constants: &ConsensusConstants,
    rules: &BaseNodeConsensusManager,
    chain_context: HeaderChainContext<'_>,
) -> Result<Option<Vec<u8>>, ValidationError> {
    if header.pow.pow_algo != PowAlgorithm::RandomXM {
        return Ok(None);
    }
    let monero_data = MoneroPowData::from_header(header, rules)?;
    let seed_height = chain_context.monero_seed_first_seen_height(db, &monero_data.randomx_key)?;
    if seed_height != 0 {
        // Saturating sub: subtraction can underflow in reorgs / rewind-blockchain command
        let seed_used_height = header.height.saturating_sub(seed_height);
        if seed_used_height > consensus_constants.max_randomx_seed_height() {
            warn!(
                target: LOG_TARGET,
                "Header {} at height {} uses a RandomX seed first seen at height {}, which is older than the allowed {} blocks",
                header.hash().to_hex(),
                header.height,
                seed_height,
                consensus_constants.max_randomx_seed_height(),
            );
            return Err(ValidationError::BlockHeaderError(
                BlockHeaderValidationError::OldSeedHash,
            ));
        }
    }
    Ok(Some(monero_data.randomx_key.to_vec()))
}

/// The largest RandomXT pow data [`check_randomxt_pow_data`] accepts.
pub(crate) const MAX_RANDOMXT_POW_DATA_SIZE: usize = 32;

/// The canonical form check for a RandomXT `pow_data`, shared by the full header validator and the cheap
/// difficulty pre-check on the block propagation path (`InboundNodeCommsHandlers::check_min_block_difficulty`).
///
/// `create_tari_mining_blob` zero pads `pow_data` out to 32 bytes before hashing, so two values share a RandomX
/// input - and therefore an achieved difficulty - exactly when one is the other extended by zero bytes, while still
/// producing different block hashes. A `pow_data` ending in `k` zero bytes consequently has `k` free, equal-work
/// variants, each one fresh to the bad-block and reconcile-dedup caches because those key on the full header hash.
///
/// The canonical form is the minimal representative of that class: empty, or ending in a non-zero byte. Nothing
/// reads these bytes - the RandomXT VM key comes from `tari_rx_vm_key_height` - so a miner that is zero padding
/// today only has to stop padding; the blob input, and so the achieved difficulty, is unchanged.
///
/// Gated on the activation height, because both the empty and the 32 byte zero padded forms are in live use. See
/// the `*_RXT_CANONICAL_POW_DATA_ACTIVATION_HEIGHT` constants for the measurements behind the chosen rule.
///
/// Callers must already have established that `pow.pow_algo` is `RandomXT`; the rule is specific to that algorithm.
pub(crate) fn check_randomxt_pow_data(
    pow: &ProofOfWork,
    require_canonical_randomxt_pow_data: bool,
) -> Result<(), PowError> {
    debug_assert_eq!(
        pow.pow_algo,
        PowAlgorithm::RandomXT,
        "check_randomxt_pow_data applied to a non-RandomXT proof of work"
    );
    if pow.pow_data.len() > MAX_RANDOMXT_POW_DATA_SIZE {
        return Err(PowError::RandomxTPowDataTooLong);
    }
    if require_canonical_randomxt_pow_data && pow.pow_data.last() == Some(&0) {
        return Err(PowError::RandomxTPowDataNotCanonical);
    }
    Ok(())
}

fn check_pow_data_inner(
    pow: &ProofOfWork,
    nonce: u64,
    cuckaroo_cycle_length: u8,
    cuckaroo_edge_bits: u8,
    require_canonical_randomxt_pow_data: bool,
) -> Result<(), ValidationError> {
    match pow.pow_algo {
        PowAlgorithm::RandomXM => {
            if nonce != 0 {
                return Err(ValidationError::BlockHeaderError(
                    BlockHeaderValidationError::InvalidNonce,
                ));
            }
            Ok(())
        },
        PowAlgorithm::RandomXT => {
            check_randomxt_pow_data(pow, require_canonical_randomxt_pow_data).map_err(ValidationError::from)
        },
        PowAlgorithm::Sha3x => {
            if !pow.pow_data.is_empty() {
                return Err(PowError::Sha3HeaderNonEmptyPowBytes.into());
            }
            Ok(())
        },
        PowAlgorithm::Cuckaroo => check_cuckaroo_pow_data(&pow.pow_data, cuckaroo_cycle_length, cuckaroo_edge_bits)
            .map_err(ValidationError::from),
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashSet;

    use super::*;
    use crate::proof_of_work::{create_tari_mining_blob, cuckaroo_pow::cuckaroo_pow_data_size};

    /// `PowData`'s bound is applied when a header is decoded, before any of the per-algorithm rules above run. If it
    /// were ever smaller than what one of those rules accepts, headers that are valid by the rules could not even be
    /// decoded, which would be a silent consensus change.
    #[test]
    fn pow_data_decode_bound_covers_every_per_algorithm_limit() {
        use tari_common::configuration::Network;
        use tari_transaction_components::{
            consensus::consensus_constants::MAX_MONERO_COINBASE_PREFIX_SIZE,
            tari_proof_of_work::PowData,
        };

        let pow_data_max = PowData::default().max_size();
        // RandomXT
        assert!(MAX_RANDOMXT_POW_DATA_SIZE <= pow_data_max);
        // RandomXM: the Monero coinbase prefix is carried inside the pow data
        assert!(MAX_MONERO_COINBASE_PREFIX_SIZE <= pow_data_max);
        // Sha3x pow data must be empty, which always fits

        for network in [
            Network::MainNet,
            Network::StageNet,
            Network::NextNet,
            Network::LocalNet,
            Network::Igor,
            Network::Esmeralda,
        ] {
            for (epoch, constants) in ConsensusConstants::for_network(network).iter().enumerate() {
                let cuckaroo =
                    cuckaroo_pow_data_size(constants.cuckaroo_cycle_length(), constants.cuckaroo_edge_bits());
                assert!(
                    cuckaroo <= pow_data_max,
                    "{network} epoch {epoch}: cuckaroo pow data ({cuckaroo}) exceeds the PowData bound"
                );
                // RandomXM pow data also carries the coinbase extra field
                assert!(
                    constants.max_extra_field_size() <= pow_data_max,
                    "{network} epoch {epoch}: max_extra_field_size exceeds the PowData bound"
                );
            }
        }
    }

    /// `AuxChainHashes` is not decoded by a node, so it backs no decode-time rule; what matters is that the aux chain
    /// merkle proof a merge mining proxy builds from the largest allowed set still decodes on the node. This pins the
    /// bound and checks that round trip for the deepest leaf.
    #[test]
    fn aux_chain_hashes_bound_fits_the_merkle_proof_bound() {
        use borsh::{BorshDeserialize, BorshSerialize};

        use crate::{
            common::MAX_AUX_CHAIN_HASHES,
            proof_of_work::monero_rx::{create_merkle_proof, tree_hash},
        };

        fn round_trip<T: BorshSerialize + BorshDeserialize>(value: &T) -> T {
            let mut buf = Vec::new();
            value.serialize(&mut buf).unwrap();
            T::deserialize(&mut buf.as_slice()).expect("the proof must decode")
        }

        const { assert!(MAX_AUX_CHAIN_HASHES == 128) };

        #[allow(clippy::cast_possible_truncation)]
        let hashes = (0..MAX_AUX_CHAIN_HASHES)
            .map(|i| monero::Hash::from([i as u8; 32]))
            .collect::<Vec<_>>();
        let root = tree_hash(&hashes).unwrap();
        for leaf in [hashes.first().unwrap(), hashes.last().unwrap()] {
            let proof = create_merkle_proof(&hashes, leaf).expect("a proof over the maximum aux chain count");
            let decoded = round_trip(&proof);
            assert_eq!(decoded.calculate_root(leaf), root);
        }
    }

    #[test]
    fn cuckaroo_pow_data_size_rounds_up_to_whole_bytes() {
        assert_eq!(cuckaroo_pow_data_size(42, 29), 153); // 1218 bits
        assert_eq!(cuckaroo_pow_data_size(8, 8), 8);
        assert_eq!(cuckaroo_pow_data_size(0, 29), 0);
    }

    /// The full validator's Cuckaroo rule is `check_cuckaroo_pow_data`, so the pre-checks that call it directly
    /// accept and reject exactly what the validator does, with the same error.
    #[test]
    fn cuckaroo_pow_data_rule_matches_check_cuckaroo_pow_data() {
        let cases = [0usize, 1, 152, 153, 154, 155, 156, 200, 1_000, 65_535]
            .into_iter()
            .map(|len| vec![1u8; len])
            .chain((0..=u8::MAX).map(|last| {
                let mut data = vec![1u8; 153];
                *data.last_mut().unwrap() = last;
                data
            }));
        for data in cases {
            let pow = ProofOfWork {
                pow_algo: PowAlgorithm::Cuckaroo,
                pow_data: data.clone().try_into().unwrap(),
            };
            let validator = check_pow_data_inner(&pow, 0, 42, 29, false).map_err(|e| match e {
                ValidationError::ProofOfWorkError(e) => e,
                e => panic!("unexpected error {e:?}"),
            });
            assert_eq!(
                validator,
                check_cuckaroo_pow_data(&data, 42, 29),
                "len {} last {:?}",
                data.len(),
                data.last()
            );
        }
    }

    #[test]
    fn test_check_pow_data_allowed_algos() {
        let allowed_algos = vec![PowAlgorithm::RandomXM];

        let pow = ProofOfWork::new(PowAlgorithm::RandomXM);
        let res = check_allowed_algos(&pow, &allowed_algos);
        assert!(res.is_ok());
        let pow = ProofOfWork::new(PowAlgorithm::RandomXT);

        let res = check_allowed_algos(&pow, &allowed_algos);
        assert!(res.is_err());
    }

    #[test]
    fn test_check_pow_data_randomxm() {
        let pow = ProofOfWork::new(PowAlgorithm::RandomXM);
        let res = check_pow_data_inner(&pow, 0, 0, 0, false);
        assert!(res.is_ok());

        let pow = ProofOfWork::new(PowAlgorithm::RandomXM);
        let res = check_pow_data_inner(&pow, 1, 0, 0, false);
        assert!(res.is_err());
    }

    #[test]
    fn test_check_pow_data_randomxt() {
        let pow = ProofOfWork::new(PowAlgorithm::RandomXT);
        let res = check_pow_data_inner(&pow, 0, 0, 0, false);
        assert!(res.is_ok());

        let pow = ProofOfWork::new(PowAlgorithm::RandomXT);
        let res = check_pow_data_inner(&pow, 0, 0, 0, false);
        assert!(res.is_ok());

        let pow = ProofOfWork {
            pow_algo: PowAlgorithm::RandomXT,
            pow_data: vec![0; 33].try_into().unwrap(),
        };
        let res = check_pow_data_inner(&pow, 0, 0, 0, false);
        assert!(res.is_err());
    }

    /// Below the activation height the historical rule is unchanged: any length up to 32 bytes is accepted.
    #[test]
    fn randomxt_pow_data_lengths_up_to_32_are_accepted_before_the_fork() {
        for len in 0..=32usize {
            let pow = ProofOfWork {
                pow_algo: PowAlgorithm::RandomXT,
                pow_data: vec![1u8; len].try_into().unwrap(),
            };
            assert!(
                check_pow_data_inner(&pow, 0, 0, 0, false).is_ok(),
                "pre-fork pow_data of {len} bytes was rejected"
            );
        }
    }

    /// The live MainNet 32 byte form, from the 500 block sample up to height 347,715: 14 meaningful bytes zero
    /// padded out to 32. This is the form the fork asks miners to stop padding.
    fn live_mainnet_pow_data() -> Vec<u8> {
        let mut v = vec![
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x35, 0xeb, 0x17, 0xfb, 0xb0, 0x3b, 0x0d, 0xfb,
        ];
        v.resize(32, 0);
        v
    }

    /// From the activation height only the minimal representative of each zero-extension class is accepted: empty,
    /// or ending in a non-zero byte. Anything ending in a zero byte is an equal-work, different-hash variant of the
    /// shorter value it extends - `create_tari_mining_blob` pads the difference away.
    #[test]
    fn randomxt_pow_data_must_be_canonical_after_the_fork() {
        let empty = ProofOfWork::new(PowAlgorithm::RandomXT);
        assert!(
            check_pow_data_inner(&empty, 0, 0, 0, true).is_ok(),
            "empty pow_data is the canonical representative of the all-zeros class and must be accepted"
        );

        // Anything ending in a non-zero byte is already minimal.
        for len in 1..=32usize {
            let pow = ProofOfWork {
                pow_algo: PowAlgorithm::RandomXT,
                pow_data: vec![1u8; len].try_into().unwrap(),
            };
            assert!(
                check_pow_data_inner(&pow, 0, 0, 0, true).is_ok(),
                "canonical pow_data of {len} bytes was rejected"
            );
        }

        // Anything ending in a zero byte is not.
        for len in 1..=32usize {
            let mut data = vec![1u8; len];
            *data.last_mut().expect("len >= 1") = 0;
            let pow = ProofOfWork {
                pow_algo: PowAlgorithm::RandomXT,
                pow_data: data.try_into().unwrap(),
            };
            let err = check_pow_data_inner(&pow, 0, 0, 0, true).unwrap_err();
            assert!(
                matches!(
                    err,
                    ValidationError::ProofOfWorkError(PowError::RandomxTPowDataNotCanonical)
                ),
                "zero-terminated pow_data of {len} bytes failed with the wrong error: {err}"
            );
        }
    }

    /// The whole point of choosing the minimal representative over "must be empty": the empty form stays valid, and
    /// the zero padded form MainNet miners emit today becomes valid again simply by dropping the padding. Both forms
    /// are in live use - 45% empty and 55% padded over the 500 blocks to height 347,715 - so a rule that accepted
    /// only one of them would orphan the other.
    #[test]
    fn both_live_randomxt_forms_have_an_accepted_canonical_value() {
        // The 45% that emit nothing are unaffected by the fork.
        let empty = ProofOfWork::new(PowAlgorithm::RandomXT);
        assert!(check_pow_data_inner(&empty, 0, 0, 0, false).is_ok());
        assert!(check_pow_data_inner(&empty, 0, 0, 0, true).is_ok());

        // The 55% that zero pad are valid before the fork and not after.
        let padded = ProofOfWork {
            pow_algo: PowAlgorithm::RandomXT,
            pow_data: live_mainnet_pow_data().try_into().unwrap(),
        };
        assert!(check_pow_data_inner(&padded, 0, 0, 0, false).is_ok());
        let err = check_pow_data_inner(&padded, 0, 0, 0, true).unwrap_err();
        assert!(matches!(
            err,
            ValidationError::ProofOfWorkError(PowError::RandomxTPowDataNotCanonical)
        ));

        // Dropping the padding is all they have to do - and it costs them no work, see
        // `stripping_the_padding_keeps_the_work` below.
        let stripped = ProofOfWork {
            pow_algo: PowAlgorithm::RandomXT,
            pow_data: live_mainnet_pow_data()
                .get(..14)
                .expect("14 <= 32")
                .to_vec()
                .try_into()
                .unwrap(),
        };
        assert!(check_pow_data_inner(&stripped, 0, 0, 0, true).is_ok());
    }

    /// Stripping the zero padding is free for a miner: same RandomX blob, so the same achieved difficulty and no
    /// re-mining. Only the block hash moves, which is exactly the malleability being removed.
    #[test]
    fn stripping_the_padding_keeps_the_work() {
        let mut header = BlockHeader::new(0);
        header.pow.pow_algo = PowAlgorithm::RandomXT;

        header.pow.pow_data = live_mainnet_pow_data().try_into().unwrap();
        let padded_blob = create_tari_mining_blob(&header);
        let padded_hash = header.hash();

        header.pow.pow_data = live_mainnet_pow_data()
            .get(..14)
            .expect("14 <= 32")
            .to_vec()
            .try_into()
            .unwrap();
        let stripped_blob = create_tari_mining_blob(&header);
        let stripped_hash = header.hash();

        assert_eq!(padded_blob, stripped_blob, "stripping the padding changed the work");
        assert_ne!(
            padded_hash, stripped_hash,
            "the two forms should be hash-distinct, that is the malleability"
        );
    }

    /// The propagation path runs the same check. `InboundNodeCommsHandlers::check_min_block_difficulty` calls this
    /// before spending a RandomX VM hash, ahead of the reconcile-dedup set and of `HeaderFullValidator`.
    #[test]
    fn the_propagation_path_check_matches_the_validator() {
        for data in [
            vec![],
            vec![0u8],
            vec![0u8; 32],
            live_mainnet_pow_data(),
            live_mainnet_pow_data().get(..14).expect("14 <= 32").to_vec(),
            vec![1u8; 33],
        ] {
            let pow = ProofOfWork {
                pow_algo: PowAlgorithm::RandomXT,
                pow_data: data.clone().try_into().unwrap(),
            };
            for gated in [false, true] {
                assert_eq!(
                    check_randomxt_pow_data(&pow, gated).is_ok(),
                    check_pow_data_inner(&pow, 0, 0, 0, gated).is_ok(),
                    "propagation and validator disagree on {data:?} at gated={gated}"
                );
            }
        }
    }

    /// Over-long `pow_data` is rejected either side of the fork, only with a different error.
    #[test]
    fn randomxt_pow_data_over_32_bytes_is_always_rejected() {
        let pow = ProofOfWork {
            pow_algo: PowAlgorithm::RandomXT,
            pow_data: vec![0; 33].try_into().unwrap(),
        };
        assert!(check_pow_data_inner(&pow, 0, 0, 0, false).is_err());
        assert!(check_pow_data_inner(&pow, 0, 0, 0, true).is_err());
    }

    /// The reason the rule exists: all 33 zero-filled `pow_data` lengths feed RandomX the same 76 byte blob, so
    /// they all have exactly the same achieved difficulty, while each one is a distinct block hash. After the fork
    /// exactly one of the 33 survives validation, which is what collapses them back into a single block.
    #[test]
    fn padded_randomxt_variants_are_one_block_with_33_hashes() {
        let mut header = BlockHeader::new(0);
        header.pow.pow_algo = PowAlgorithm::RandomXT;

        let mut blobs = HashSet::new();
        let mut hashes = HashSet::new();
        let mut accepted = 0usize;
        for len in 0..=32usize {
            header.pow.pow_data = vec![0u8; len].try_into().unwrap();
            blobs.insert(create_tari_mining_blob(&header));
            hashes.insert(header.hash());
            if check_pow_data_inner(&header.pow, 0, 0, 0, true).is_ok() {
                accepted += 1;
            }
        }

        assert_eq!(blobs.len(), 1, "the padded variants do not all share a RandomX input");
        assert_eq!(
            blobs.iter().next().expect("just asserted non-empty").len(),
            76,
            "the mining blob is not the 76 byte XMRig blob"
        );
        assert_eq!(
            hashes.len(),
            33,
            "the padded variants do not all have distinct block hashes"
        );
        assert_eq!(accepted, 1, "more than the canonical variant survives the fork rule");
    }

    /// The malleability class is exactly "zero extension to 32 bytes", not "any length below 32". This is what the
    /// chain scan ran into: the `pow_data` MainNet miners actually emit is 32 bytes with a long zero tail, so it
    /// already has one equal-work variant per trailing zero byte. The bytes are real data here, taken from MainNet
    /// block 347,657, so the test also pins that a non-empty `pow_data` is the live form rather than a curiosity.
    #[test]
    fn the_equal_work_variants_are_the_zero_extensions() {
        let mut header = BlockHeader::new(0);
        header.pow.pow_algo = PowAlgorithm::RandomXT;
        // MainNet block 347,657: six zero bytes, eight bytes of miner data, then eighteen trailing zeros.
        let mut live = vec![0u8; 32];
        live.splice(6..14, [0x36, 0x02, 0x59, 0x69, 0x91, 0x57, 0x39, 0x44]);
        let trailing_zeros = live.iter().rev().take_while(|b| **b == 0).count();
        assert_eq!(trailing_zeros, 18);

        // Truncating any of the trailing zeros leaves the mining blob - and so the work - untouched.
        let mut blobs = HashSet::new();
        let mut hashes = HashSet::new();
        for len in (32 - trailing_zeros)..=32 {
            header.pow.pow_data = live.get(..len).expect("len <= 32").to_vec().try_into().unwrap();
            blobs.insert(create_tari_mining_blob(&header));
            hashes.insert(header.hash());
        }
        assert_eq!(blobs.len(), 1, "truncating a trailing zero changed the RandomX input");
        assert_eq!(
            hashes.len(),
            19,
            "truncating a trailing zero did not change the block hash"
        );

        // After the fork exactly one of those 19 survives: the 14 byte minimal representative.
        let accepted = ((32 - trailing_zeros)..=32)
            .filter(|len| {
                let pow = ProofOfWork {
                    pow_algo: PowAlgorithm::RandomXT,
                    pow_data: live.get(..*len).expect("len <= 32").to_vec().try_into().unwrap(),
                };
                check_pow_data_inner(&pow, 0, 0, 0, true).is_ok()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            accepted,
            vec![32 - trailing_zeros],
            "exactly the minimal form should survive"
        );

        // Truncating a non-zero byte does change the work, so it is not a free variant.
        header.pow.pow_data = live.get(..13).expect("13 <= 32").to_vec().try_into().unwrap();
        let shortened = create_tari_mining_blob(&header);
        assert!(
            !blobs.contains(&shortened),
            "truncating a non-zero byte kept the same input"
        );
    }

    #[test]
    fn test_check_pow_data_sha3x() {
        let pow = ProofOfWork::new(PowAlgorithm::Sha3x);
        let res = check_pow_data_inner(&pow, 0, 0, 0, false);
        assert!(res.is_ok());

        let pow = ProofOfWork {
            pow_algo: PowAlgorithm::Sha3x,
            pow_data: vec![1].try_into().unwrap(),
        };
        let res = check_pow_data_inner(&pow, 0, 0, 0, false);
        assert!(res.is_err());
    }

    #[test]
    fn test_check_pow_data_cuckaroo_data_size_mismatch() {
        let pow = ProofOfWork {
            pow_algo: PowAlgorithm::Cuckaroo,
            pow_data: vec![0u8; 10].try_into().unwrap(),
        };
        // Check with multiple of 8
        let res = check_pow_data_inner(&pow, 0, 10, 8, false);
        assert!(res.is_ok());

        // Check with non-multiple of 8
        let pow = ProofOfWork {
            pow_algo: PowAlgorithm::Cuckaroo,
            pow_data: vec![0u8; 11].try_into().unwrap(),
        };
        let res = check_pow_data_inner(&pow, 0, 9, 9, false);
        assert!(res.is_ok());

        // Check now with invalid data.

        let pow = ProofOfWork {
            pow_algo: PowAlgorithm::Cuckaroo,
            pow_data: vec![0u8; 11].try_into().unwrap(),
        };
        let res = check_pow_data_inner(&pow, 0, 10, 8, false);
        assert!(res.is_err());

        let pow = ProofOfWork {
            pow_algo: PowAlgorithm::Cuckaroo,
            pow_data: vec![0u8; 12].try_into().unwrap(),
        };
        let res = check_pow_data_inner(&pow, 0, 9, 9, false);
        assert!(res.is_err());
    }

    #[test]
    fn a_header_with_malformed_pow_data_is_named_by_block_and_algorithm() {
        use tari_common::configuration::Network;
        use tari_transaction_components::tari_proof_of_work::PowData;

        let rules = BaseNodeConsensusManager::builder(Network::LocalNet).build().unwrap();
        let mut header = BlockHeader::new(0);
        header.pow.pow_algo = PowAlgorithm::Sha3x;
        header.pow.pow_data = PowData::try_from(vec![0xff; PowData::default().max_size()]).unwrap();

        let err = check_pow_data(&header, rules.consensus_constants(0)).unwrap_err();
        assert!(
            matches!(
                err,
                ValidationError::ProofOfWorkError(PowError::Sha3HeaderNonEmptyPowBytes)
            ),
            "{err:?}"
        );
        let ban_reason = err.get_ban_reason().expect("malformed pow data is bannable").reason;
        let message = pow_rejection_message(&header.hash(), &header, false, &err);

        assert!(message.contains(&header.hash().to_hex()), "{message}");
        assert!(message.contains("Sha3"), "{message}");
        // The ban record carries the same error, which is what ties the two together
        assert!(message.contains(&ban_reason), "{message}");
        assert!(message.len() < 512, "{} bytes: {message}", message.len());
    }

    /// With no target given, the proof of work is checked under the VM key of the chain context, which is the key of
    /// the header's own chain, not under the main chain's
    #[test]
    fn the_vm_key_of_the_chain_context_is_used_without_a_target() {
        use tari_common::configuration::Network;

        use crate::{
            proof_of_work::{randomx_factory::RandomXFactory, tari_randomx_difficulty},
            test_helpers::blockchain::create_custom_blockchain,
        };

        let rules = BaseNodeConsensusManager::builder(Network::LocalNet).build().unwrap();
        let factory = RandomXFactory::default();
        let validator =
            HeaderFullValidator::new(rules.clone(), DifficultyCalculator::new(rules.clone(), factory.clone()));
        let db = create_custom_blockchain(rules);
        let genesis = db.fetch_chain_header(0).unwrap();
        let main_key = *genesis.hash();
        let fork_key = FixedHash::from([1u8; 32]);
        let mut header = BlockHeader::from_previous(genesis.header());
        header.timestamp = genesis.header().timestamp.checked_add(EpochTime::from(120)).unwrap();
        header.kernel_mmr_size = header.kernel_mmr_size.saturating_add(1);
        header.pow = ProofOfWork::new(PowAlgorithm::RandomXT);
        // Most keys give the lowest difficulty, so find a nonce for which the two keys do not
        while tari_randomx_difficulty(&header, &factory, &fork_key).unwrap() ==
            tari_randomx_difficulty(&header, &factory, &main_key).unwrap()
        {
            header.nonce += 1;
        }

        let access = db.db_read_access().unwrap();
        let validated = validator
            .validate(
                &*access,
                &header,
                genesis.header(),
                &[genesis.header().timestamp],
                None,
                HeaderChainContext::candidate_chain(fork_key, 0, None),
            )
            .unwrap();

        assert_eq!(
            validated.achieved_target.achieved(),
            tari_randomx_difficulty(&header, &factory, &fork_key).unwrap()
        );
    }

    #[test]
    fn a_header_whose_kernel_mmr_size_grows_passes() {
        let mut prev = BlockHeader::new(0);
        prev.kernel_mmr_size = 10;
        for size in [11, 25] {
            let mut header = BlockHeader::from_previous(&prev);
            header.kernel_mmr_size = size;
            check_kernel_mmr_size(&header, &prev).unwrap();
        }
    }

    #[test]
    fn a_header_whose_kernel_mmr_size_does_not_grow_is_rejected() {
        use tari_transaction_components::BanPeriod;

        let mut prev = BlockHeader::new(0);
        prev.kernel_mmr_size = 10;
        for size in [10, 9, 0] {
            let mut header = BlockHeader::from_previous(&prev);
            header.kernel_mmr_size = size;
            let err = check_kernel_mmr_size(&header, &prev).unwrap_err();
            assert!(
                matches!(
                    err,
                    ValidationError::BlockHeaderError(BlockHeaderValidationError::KernelMmrSizeNotIncreasing {
                        prev: 10,
                        actual,
                    }) if actual == size
                ),
                "{err}"
            );
            assert_eq!(err.get_ban_reason().unwrap().ban_duration, BanPeriod::Long);
        }
    }

    /// A LocalNet validator and database, and a Sha3x header on top of genesis with the given kernel MMR size
    fn kernel_mmr_size_setup(
        kernel_mmr_size: u64,
    ) -> (
        HeaderFullValidator,
        crate::chain_storage::BlockchainDatabase<crate::test_helpers::blockchain::TempDatabase>,
        BlockHeader,
    ) {
        use tari_common::configuration::Network;

        use crate::test_helpers::blockchain::create_custom_blockchain;

        let rules = BaseNodeConsensusManager::builder(Network::LocalNet).build().unwrap();
        let validator = HeaderFullValidator::new(
            rules.clone(),
            DifficultyCalculator::new(rules.clone(), Default::default()),
        );
        let db = create_custom_blockchain(rules);
        let genesis = db.fetch_chain_header(0).unwrap();
        let mut header = BlockHeader::from_previous(genesis.header());
        header.timestamp = genesis.header().timestamp.checked_add(EpochTime::from(120)).unwrap();
        header.pow = ProofOfWork::new(PowAlgorithm::Sha3x);
        header.kernel_mmr_size = kernel_mmr_size;
        (validator, db, header)
    }

    fn validate_on_genesis(
        kernel_mmr_size: u64,
        target_difficulty: Option<AdjustedTarget>,
    ) -> Result<ValidatedHeader, ValidationError> {
        let (validator, db, header) = kernel_mmr_size_setup(kernel_mmr_size);
        let genesis = db.fetch_chain_header(0).unwrap();
        let access = db.db_read_access().unwrap();
        validator.validate(
            &*access,
            &header,
            genesis.header(),
            &[genesis.header().timestamp],
            target_difficulty,
            HeaderChainContext::candidate_chain(*genesis.hash(), 0, None),
        )
    }

    #[test]
    fn the_validator_checks_the_kernel_mmr_size() {
        let (_, db, _) = kernel_mmr_size_setup(0);
        let genesis_size = db.fetch_chain_header(0).unwrap().header().kernel_mmr_size;

        validate_on_genesis(genesis_size + 1, None).unwrap();
        validate_on_genesis(genesis_size + 5, None).unwrap();
        let err = validate_on_genesis(genesis_size, None).unwrap_err();
        assert!(
            matches!(
                err,
                ValidationError::BlockHeaderError(BlockHeaderValidationError::KernelMmrSizeNotIncreasing { .. })
            ),
            "{err}"
        );
    }

    /// A header with no proof of work fails on that, so it is never recorded as bad for its kernel MMR size
    #[test]
    fn proof_of_work_is_checked_before_the_kernel_mmr_size() {
        use tari_transaction_components::tari_proof_of_work::Difficulty;

        let (_, db, _) = kernel_mmr_size_setup(0);
        let genesis_size = db.fetch_chain_header(0).unwrap().header().kernel_mmr_size;

        let err = validate_on_genesis(genesis_size, Some(AdjustedTarget::unadjusted(Difficulty::max()))).unwrap_err();
        assert!(
            matches!(
                err,
                ValidationError::BlockHeaderError(BlockHeaderValidationError::ProofOfWorkError(
                    PowError::AchievedDifficultyTooLow { .. }
                ))
            ),
            "{err}"
        );
    }

    #[test]
    fn test_check_pow_data_cuckaroo_non_zero_padding() {
        let pow = ProofOfWork {
            pow_algo: PowAlgorithm::Cuckaroo,
            pow_data: vec![128u8; 11].try_into().unwrap(),
        };
        let res = check_pow_data_inner(&pow, 0, 9, 9, false);
        assert!(res.is_err());
    }
}
