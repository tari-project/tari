//  Copyright 2021, The Tari Project
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
use std::iter;

use log::*;
use monero::{
    VarInt,
    blockdata::transaction::{ExtraField, RawExtraField, SubField},
    consensus,
    consensus::Encodable,
    cryptonote::hash::Hashable,
};
use primitive_types::U256;
use sha2::{Digest, Sha256};
use tari_common_types::types::FixedHash;
use tari_node_components::blocks::BlockHeader;
use tari_transaction_components::{
    consensus::consensus_constants::MAX_MONERO_COINBASE_PREFIX_SIZE,
    tari_proof_of_work::Difficulty,
};
use tari_utilities::hex::HexError;
use tiny_keccak::{Hasher, Keccak};

use super::{
    error::MergeMineError,
    fixed_array::FixedByteArray,
    merkle_tree::{create_merkle_proof, expected_branch_len, tree_hash},
    pow_data::{CoinbasePrefix, CoinbasePrefixMode, CoinbaseTxPrefix, MoneroPowData},
};
use crate::{
    common::AuxChainHashes,
    consensus::BaseNodeConsensusManager,
    proof_of_work::{
        monero_rx::merkle_tree_parameters::MerkleTreeParameters,
        randomx_factory::{RandomXFactory, RandomXVMInstance},
    },
};

pub const LOG_TARGET: &str = "c::pow::monero_rx";

///  Calculates the achieved Monero difficulty for the `BlockHeader`. An error is returned if the BlockHeader does not
/// contain valid Monero PoW data.
pub fn monero_randomx_difficulty(
    header: &BlockHeader,
    randomx_factory: &RandomXFactory,
    genesis_block_hash: &FixedHash,
    consensus: &BaseNodeConsensusManager,
) -> Result<Difficulty, MergeMineError> {
    monero_randomx_difficulty_at_rules_height(header, randomx_factory, genesis_block_hash, consensus, header.height)
}

/// As [`monero_randomx_difficulty`], but selects the consensus rules from `rules_height` instead of from the
/// header's own (possibly unverified) `height` field. See [`verify_header_at_rules_height`].
pub fn monero_randomx_difficulty_at_rules_height(
    header: &BlockHeader,
    randomx_factory: &RandomXFactory,
    genesis_block_hash: &FixedHash,
    consensus: &BaseNodeConsensusManager,
    rules_height: u64,
) -> Result<Difficulty, MergeMineError> {
    let monero_pow_data = verify_header_at_rules_height(header, genesis_block_hash, consensus, rules_height)?;
    trace!(target: LOG_TARGET, "Valid Monero data: {monero_pow_data}");
    let blockhashing_blob = monero_pow_data.to_blockhashing_blob();
    let vm = randomx_factory.create(monero_pow_data.randomx_key(), None, None)?;
    get_random_x_difficulty(&blockhashing_blob, &vm).map(|(diff, _)| diff)
}

/// Creates the 76-byte XMRig-compatible mining blob for Tari RandomXT.
///
/// The blob format is:
/// ```text
/// | 1 byte | 1 byte | 1 byte | 32 bytes | 8 bytes | 1 byte | 32 bytes |
/// | major  | minor  | ts     | mining_hash | nonce (big-endian u64) | pow_algo | pow_data (padded to 32 bytes) |
/// ```
///
/// The 8-byte nonce is split so that:
/// - High 4 bytes (offset 35): per-thread extra nonce (written at `reserved_offset` by XMRig)
/// - Low 4 bytes (offset 39): main nonce iterated by XMRig (at the standard Monero nonce offset)
///
/// XMRig should be configured with `"coin": "tari"` and `"daemon": true` to use this blob.
///
/// The zero padding of `pow_data` out to 32 bytes is load-bearing for wire compatibility and must not be removed:
/// every RandomXT block ever mined was hashed as a 76 byte blob, and dropping the padding would change the RandomX
/// input of all of them. It does mean the blob cannot tell a `pow_data` from its own zero extension, so a block
/// whose `pow_data` ends in `k` zero bytes has `k` equal-work variants that differ only in their block hash. That
/// is what `check_randomxt_pow_data` closes from the fork height on, by requiring the minimal representative of
/// each zero-extension class: a RandomXT `pow_data` must be empty or end in a non-zero byte.
pub fn create_tari_mining_blob(header: &BlockHeader) -> Vec<u8> {
    let mut blob = vec![0u8; 3];
    blob.extend_from_slice(header.mining_hash().as_slice());
    let nonce = header.nonce.to_be_bytes();
    blob.extend_from_slice(&nonce);
    let mut pow_bytes = header.pow.to_bytes();
    if pow_bytes.len() < 33 {
        pow_bytes.resize(33, 0)
    }
    blob.extend_from_slice(pow_bytes.get(0..33).expect("This should exist"));
    blob
}

pub fn tari_randomx_difficulty(
    header: &BlockHeader,
    randomx_factory: &RandomXFactory,
    vm_key: &FixedHash,
) -> Result<Difficulty, MergeMineError> {
    let vm = randomx_factory.create(vm_key.as_slice(), None, None)?;
    let blob = create_tari_mining_blob(header);
    get_random_x_difficulty(&blob, &vm).map(|(diff, _)| diff)
}

/// Calculate the RandomX mining hash using the virtual machine together with the achieved difficulty
fn get_random_x_difficulty(input: &[u8], vm: &RandomXVMInstance) -> Result<(Difficulty, Vec<u8>), MergeMineError> {
    let hash = vm.calculate_hash(input)?;
    debug!(target: LOG_TARGET, "RandomX Hash: {hash:?}");
    let difficulty = Difficulty::little_endian_difficulty(&hash)?;
    Ok((difficulty, hash))
}

// Parsing an extra field from bytes will always return an extra field with sub-fields that could be read, even if it
// does not represent the original extra field. As per Monero consensus rules, an error here will not represent a
// failure to deserialize a block, so no need to error here.
fn parse_extra_field_truncate_on_error(raw_extra_field: &RawExtraField) -> ExtraField {
    match ExtraField::try_parse(raw_extra_field) {
        Ok(val) => val,
        Err(val) => {
            warn!(
                target: LOG_TARGET,
                "Some sub-fields could not be parsed successfully from the Monero coinbase extra field and will be \
                excluded"
            );
            val
        },
    }
}

/// Validates the monero data contained in the given header, making these assetions:
/// 1. The MoneroPowData is well-formed (i.e. can be deserialized)
/// 1. The header's merge mining hash is included in the coinbase extra field
/// 1. The merkle proof and coinbase hash produce a matching merkle root
///
/// If these assertions pass, a valid `MoneroPowData` instance is returned
pub fn verify_header(
    header: &BlockHeader,
    genesis_block_hash: &FixedHash,
    consensus: &BaseNodeConsensusManager,
) -> Result<MoneroPowData, MergeMineError> {
    verify_header_at_rules_height(header, genesis_block_hash, consensus, header.height)
}

/// As [`verify_header`], but selects the consensus rules from `rules_height` rather than from `header.height`.
///
/// Consensus validation always passes `header.height`, because a header being validated is already linked to a
/// parent and its height is therefore a fact. The gossip pre-validation gate cannot: it is looking at an
/// unlinked header whose `height` is whatever the announcing peer wrote, and picking the rule set from that lets
/// the peer pick its own rules. That caller passes a height it can corroborate instead.
pub fn verify_header_at_rules_height(
    header: &BlockHeader,
    genesis_block_hash: &FixedHash,
    consensus: &BaseNodeConsensusManager,
    rules_height: u64,
) -> Result<MoneroPowData, MergeMineError> {
    let monero_data = MoneroPowData::from_header_at_rules_height(header, consensus, rules_height)?;
    let expected_merge_mining_hash = header.merge_mining_hash();
    let extra_field = ExtraField::try_parse(&monero_data.coinbase_tx_extra);
    let extra_field = extra_field.unwrap_or_else(|ex_field| {
        trace!(target: LOG_TARGET, "Error deserializing, Monero extra field");
        ex_field
    });
    debug!(target: LOG_TARGET, "Extra field: {extra_field:?}");
    // Check that the Tari MM hash is found in the Monero coinbase transaction
    // and that only 1 Tari header is found

    // `rules_height`, not `header.height`: this function's contract is that the *caller* names the height the
    // rules are selected from, and the gossip pre-validation gate is the caller that cannot trust the header's own
    // field. Reading `header.height` here would have left a peer able to name a pre-activation height and be handed
    // the pre-fork aux-chain proof check, which is half of what the gate exists to stop. On the consensus path the
    // two are the same value, so nothing there changes.
    let enforce_depth_binding = consensus
        .consensus_constants(rules_height)
        .aux_chain_merkle_proof_depth_binding();

    // GHSA-3qmx-q9pv-f3m4 item 3: below the activation height the merge mining parameters are decoded with the
    // pre-fork decoder that the historical chain was validated with; at and above it only the canonical encoding is
    // accepted.
    let strict_merkle_tree_params = consensus
        .consensus_constants(rules_height)
        .strict_merkle_tree_parameter_decoding();

    // Held back so the shadow check runs only once the tag has actually passed merge mining verification. See
    // `shadow_check_merkle_tree_params` for why reporting earlier would be both false and attacker-forgeable.
    let mut shadow_candidate: Option<VarInt> = None;
    // The outcome of the *single* permitted merge mining tag, or `None` if the coinbase carried no tag at all.
    // Keeping the reason instead of collapsing it to a bool is what makes a rejection diagnosable: every failure
    // below used to be reported to the operator, and to the peer ban, as "Expected merge mining tag was not
    // found", which was actively false whenever a tag *was* found and simply did not check out.
    let mut aux_chain_outcome: Option<Result<(), MergeMineError>> = None;
    let mut already_seen_mmfield = false;
    for item in extra_field.0 {
        if let SubField::MergeMining(depth, merge_mining_hash) = item {
            if already_seen_mmfield {
                return Err(MergeMineError::ValidationError(
                    "More than one merge mining tag found in coinbase".to_string(),
                ));
            }
            already_seen_mmfield = true;
            shadow_candidate = Some(depth.clone());
            aux_chain_outcome = Some(check_aux_chains(
                &monero_data,
                depth,
                &merge_mining_hash,
                &expected_merge_mining_hash,
                genesis_block_hash,
                enforce_depth_binding,
                strict_merkle_tree_params,
                header.height,
            ));
        }
    }

    match aux_chain_outcome {
        Some(Ok(())) => {},
        Some(Err(err)) => return Err(err),
        None => {
            return Err(MergeMineError::ValidationError(
                "Expected merge mining tag was not found in Monero coinbase transaction".to_string(),
            ));
        },
    }

    if !monero_data.is_coinbase_valid_merkle_root() {
        return Err(MergeMineError::InvalidMerkleRoot);
    }

    // Only now, with the tag matched and the coinbase merkle root checked, is it true that this tag passed merge
    // mining verification and would have been rejected after the fork. Reporting any earlier would log a claim the
    // surrounding code has not established, and would let any peer forge the signal for free.
    if let Some(params) = shadow_candidate.filter(|_| !strict_merkle_tree_params) {
        shadow_check_merkle_tree_params(&params, header.height, &header.hash());
    }

    Ok(monero_data)
}

/// GHSA-3qmx-q9pv-f3m4 item 3, shadow mode. Below a network's activation height the strict decoder is run purely
/// for its diagnostic value: the answer is discarded here and never reaches a caller, so this cannot change what a
/// pre-fork node accepts. That property is structural - the function returns `()` - and it is the single most
/// important thing about it.
///
/// The point is coverage over the tag-to-flag-day window. A point-in-time scan of the chain can only see what has
/// already been mined; a newly deployed non-conformant block producer would appear after the scan and before the
/// fork, which is exactly the window in which nobody would otherwise notice. Cost is one extra decode per
/// merge-mined block, far below the RandomX hash the same code path already performs.
///
/// The caller must only reach here once the tag has matched and the coinbase merkle root has been checked. Warning
/// earlier would be wrong twice over. It would state that a block passed when that had not yet been decided; and
/// because `verify_header` runs before any proof of work is verified, any peer could manufacture "a non-conformant
/// producer is live" reports for free. That matters because this signal, together with the MainNet scan, is what
/// the activation height is supposed to be judged on - a forgeable input to that decision is worse than none. The
/// block hash is logged so a report can be checked against the chain rather than taken on trust.
fn shadow_check_merkle_tree_params(merge_mining_params: &VarInt, height: u64, block_hash: &FixedHash) {
    if let Err(e) = MerkleTreeParameters::from_varint(merge_mining_params.clone()) {
        warn!(
            target: LOG_TARGET,
            "GHSA-3qmx-q9pv-f3m4-SHADOW: NO ACTION TAKEN, THIS BLOCK PASSED MERGE MINING VERIFICATION. The block at \
             height {height} ({block_hash}) carries merge mining parameters {} that the pre-fork decoder accepts but \
             that are not a canonical encoding, so it WOULD BE REJECTED once strict decoding activates on this \
             network. {e}",
            merge_mining_params.0
        );
    }
}

/// Checks that the aux-chain merkle proof in `monero_data` really binds this Tari header to the aux-chain merkle
/// root carried in the Monero coinbase's merge mining tag.
///
/// Returns `Ok(())` when it does, and a `MergeMineError::ValidationError` naming the actual reason when it does
/// not. The reason is the point: this used to return a bare `bool`, the detailed message built inside
/// `calculate_root_with_pos` was discarded, and `verify_header` reported *every* failure here as "Expected merge
/// mining tag was not found in Monero coinbase transaction" - a message that is false whenever a tag was found
/// and parsed, which is every path through this function. Operators saw nothing to act on and the peer ban reason
/// named the wrong fault.
///
/// The accept/reject verdict is unchanged on both sides of the fork; only the message and the logging are new.
///
/// `strict_merkle_tree_params` selects the merge mining parameter decoder (GHSA-3qmx-q9pv-f3m4 item 3). On the
/// strict path a varint that is not the canonical encoding of the parameters it decodes to is rejected, and that
/// rejection is reported as `MergeMineError::MerkleTreeParamsError` rather than as a `ValidationError`, because it
/// is the one failure in this function that must never ban the relaying peer. See the comment at the decode site.
// Ristretto point/scalar arithmetic, not integer arithmetic: cannot overflow.
#[allow(clippy::arithmetic_side_effects)]
fn check_aux_chains(
    monero_data: &MoneroPowData,
    merge_mining_params: VarInt,
    aux_chain_merkle_root: &monero::Hash,
    tari_hash: &FixedHash,
    tari_genesis_block_hash: &FixedHash,
    enforce_depth_binding: bool,
    strict_merkle_tree_params: bool,
    height: u64,
) -> Result<(), MergeMineError> {
    let t_hash = monero::Hash::from_slice(tari_hash.as_slice());
    let proof = &monero_data.aux_chain_merkle_proof;
    let branch_len = proof.branch().len();
    let path_bitmap = proof.path();
    if merge_mining_params == VarInt(0) {
        // we interpret 0 as there is only 1 chain, tari.
        // A single chain is a zero-depth tree: the Tari hash *is* the root, so the branch must be empty. Taking
        // this shortcut with a non-empty branch would let the same Monero proof of work stand behind arbitrarily
        // many distinct Tari headers. The bitmap must be zero for the same reason `check_coinbase_path` requires
        // it of the coinbase proof: with an empty branch nothing ever reads it, so any bit set in it is free
        // block hash malleability.
        if t_hash == *aux_chain_merkle_root && (!enforce_depth_binding || (branch_len == 0 && path_bitmap == 0)) {
            return Ok(());
        }
    }
    let merkle_tree_params = if strict_merkle_tree_params {
        match MerkleTreeParameters::from_varint(merge_mining_params) {
            Ok(params) => params,
            Err(e) => {
                // A decode failure is reported as its own error rather than folded into the generic validation
                // failure below. The tag *was* found; it decoded and was then rejected as non-canonical, and a
                // message about the proof or the tag being absent would send whoever is on call looking for the
                // wrong thing at the exact moment this fork bites. The height lives in this log line and not in
                // the error, because the error variant cannot carry it.
                warn!(
                    target: LOG_TARGET,
                    "Merge mining parameters in the Monero coinbase of the block at height {height} could not be \
                     decoded: {e}"
                );
                // Deliberately *not* a `ValidationError`: `get_ban_reason` maps `MerkleTreeParamsError` to `None`,
                // which is the pre-existing judgement that merge mining parameter problems are not peer
                // misbehaviour, and it is the right one here. The relaying peer did not author this varint and
                // cannot alter it - it is committed to by the Monero proof of work - and it has no way to know
                // that we disagree with it about the activation height. Banning on this turns a height
                // disagreement into an eclipse of the upgraded minority: it would ban the honest, not yet upgraded
                // peers relaying the majority chain, and because ban/no-ban is a clean binary signal (the pre-fork
                // decoder is infallible and can never produce this error) it would also hand any observer a free
                // remote probe for which build a node is running.
                return Err(e.into());
            },
        }
    } else {
        MerkleTreeParameters::from_varint_legacy(merge_mining_params)
    };
    let number_of_chains = merkle_tree_params.number_of_chains();
    if number_of_chains == 0 {
        let reason = format!(
            "Aux chain merkle proof rejected: the merge mining tag declares zero aux chains (tari hash {tari_hash}, \
             height {height}, branch length {branch_len}, path bitmap {path_bitmap:#010x})"
        );
        warn!(target: LOG_TARGET, "{reason}");
        return Err(MergeMineError::ValidationError(reason));
    }
    let hash_position = U256::from_little_endian(
        &Sha256::new()
            .chain_update(tari_genesis_block_hash)
            .chain_update(merkle_tree_params.aux_nonce().to_le_bytes())
            .chain_update((109_u8).to_le_bytes())
            .finalize(),
    )
    .low_u32() %
        u32::from(number_of_chains);

    let expected_len = expected_branch_len(number_of_chains, hash_position);
    // Everything an operator needs to tell a forgery from a bug: all of it is either derived from the genesis
    // hash and the aux nonce, or read straight off the wire, so none of it can be misleading. Deliberately a
    // closure rather than a value: this runs for every merge mined header, and the overwhelming majority of them
    // are honest, so the accept path must not pay for a string it never uses.
    let context = || {
        format!(
            "tari hash {tari_hash}, height {height}, chains {number_of_chains}, aux nonce {}, expected position \
             {hash_position}, branch length {branch_len} (expected {expected_len}), path bitmap {path_bitmap:#010x}, \
             depth binding {enforce_depth_binding}",
            merkle_tree_params.aux_nonce(),
        )
    };

    // Bind the proof's depth to the chain count *before* trusting the proof for anything. `hash_position` is
    // derived from the genesis hash and the aux nonce alone, so the expected branch length is known independently
    // of anything the miner supplied.
    if enforce_depth_binding && branch_len != expected_len {
        let reason = format!(
            "Aux chain merkle proof rejected: branch length does not match the chain count; {}",
            context()
        );
        warn!(target: LOG_TARGET, "{reason}");
        return Err(MergeMineError::ValidationError(reason));
    }

    let (merkle_root, pos) = match proof.calculate_root_with_pos(&t_hash, number_of_chains, enforce_depth_binding) {
        Ok(val) => val,
        Err(err) => {
            let reason = format!("Aux chain merkle proof rejected: {err}; {}", context());
            warn!(target: LOG_TARGET, "{reason}");
            return Err(MergeMineError::ValidationError(reason));
        },
    };
    if hash_position != pos {
        let reason = format!(
            "Aux chain merkle proof rejected: the path bitmap encodes leaf position {pos}, but this chain is assigned \
             position {hash_position}; {}",
            context()
        );
        warn!(target: LOG_TARGET, "{reason}");
        return Err(MergeMineError::ValidationError(reason));
    }

    if merkle_root != *aux_chain_merkle_root {
        let reason = format!(
            "Aux chain merkle proof rejected: the proof reconstructs root {merkle_root} but the merge mining tag \
             commits to {aux_chain_merkle_root}; {}",
            context()
        );
        warn!(target: LOG_TARGET, "{reason}");
        return Err(MergeMineError::ValidationError(reason));
    }

    Ok(())
}

/// Extracts the Monero block hash from the coinbase transaction's extra field
pub fn extract_aux_merkle_root_from_block(monero: &monero::Block) -> Result<Option<monero::Hash>, MergeMineError> {
    // When we extract the merge mining hash, we do not care if the extra field can be parsed without error.
    let extra_field = parse_extra_field_truncate_on_error(&monero.miner_tx.prefix.extra);

    // Only one merge mining tag is allowed
    let merge_mining_hashes: Vec<monero::Hash> = extra_field
        .0
        .iter()
        .filter_map(|item| {
            if let SubField::MergeMining(_depth, merge_mining_hash) = item {
                Some(*merge_mining_hash)
            } else {
                None
            }
        })
        .collect();
    if merge_mining_hashes.len() > 1 {
        return Err(MergeMineError::ValidationError(
            "More than one merge mining tag found in coinbase".to_string(),
        ));
    }

    if let Some(merge_mining_hash) = merge_mining_hashes.into_iter().next() {
        Ok(Some(merge_mining_hash))
    } else {
        Ok(None)
    }
}

/// Deserializes the given hex-encoded string into a Monero block
pub fn deserialize_monero_block_from_hex<T>(data: T) -> Result<monero::Block, MergeMineError>
where T: AsRef<[u8]> {
    let bytes = hex::decode(data).map_err(|_| HexError::HexConversionError {})?;
    let obj = consensus::deserialize::<monero::Block>(&bytes)
        .map_err(|e| MergeMineError::ValidationError(format!("blocktemplate blob invalid: {e}")))?;
    Ok(obj)
}

/// Serializes the given Monero block into a hex-encoded string
pub fn serialize_monero_block_to_hex(obj: &monero::Block) -> Result<String, MergeMineError> {
    let data = consensus::serialize::<monero::Block>(obj);
    let bytes = hex::encode(data);
    Ok(bytes)
}

/// Packages the consensus encoded coinbase transaction prefix into the wire format `mode` calls for.
///
/// GHSA-3qmx-q9pv-f3m4: in [`CoinbasePrefixMode::Derived`] the bytes themselves travel, and the verifier derives
/// the sponge from them. [`CoinbasePrefixMode::Legacy`] reproduces exactly what merge miners sent before the fork -
/// the sponge with the prefix already absorbed - so that pow data for pre-fork heights is still constructible.
fn coinbase_prefix_from(encoded_prefix: &[u8], mode: CoinbasePrefixMode) -> Result<CoinbasePrefix, MergeMineError> {
    match mode {
        CoinbasePrefixMode::Legacy => {
            let mut keccak = Keccak::v256();
            keccak.update(encoded_prefix);
            Ok(CoinbasePrefix::Legacy(keccak))
        },
        CoinbasePrefixMode::Derived => {
            let prefix = CoinbaseTxPrefix::try_from(encoded_prefix.to_vec()).map_err(|_| {
                MergeMineError::CoinbasePrefixTooLarge {
                    size: encoded_prefix.len(),
                    max: MAX_MONERO_COINBASE_PREFIX_SIZE,
                }
            })?;
            Ok(CoinbasePrefix::Prefix(prefix))
        },
    }
}

/// Constructs the Monero PoW data from the given block and seed.
///
/// `coinbase_prefix_mode` must be the format the *Tari* header this pow data is going into is at: below the
/// GHSA-3qmx-q9pv-f3m4 activation height the node only accepts the legacy sponge format, at and above it only the
/// coinbase prefix format. Use [`CoinbasePrefixMode::for_height`] to pick it.
pub fn construct_monero_data(
    block: monero::Block,
    seed: FixedByteArray,
    ordered_aux_chain_hashes: AuxChainHashes,
    tari_hash: FixedHash,
    coinbase_prefix_mode: CoinbasePrefixMode,
) -> Result<MoneroPowData, MergeMineError> {
    let hashes = create_ordered_transaction_hashes_from_block(&block);
    let root = tree_hash(&hashes)?;
    let hash = hashes.first().ok_or(MergeMineError::ValidationError(
        "No hashes for merkle proof".to_string(),
    ))?;
    let coinbase_merkle_proof = create_merkle_proof(&hashes, hash).ok_or_else(|| {
        MergeMineError::ValidationError(
            "create_merkle_proof returned None because the block had no coinbase (which is impossible because the \
             Block type does not allow that)"
                .to_string(),
        )
    })?;
    let coinbase = block.miner_tx.clone();

    let mut encoder_prefix = Vec::new();
    coinbase
        .prefix
        .version
        .consensus_encode(&mut encoder_prefix)
        .map_err(|e| MergeMineError::SerializeError(e.to_string()))?;
    coinbase
        .prefix
        .unlock_time
        .consensus_encode(&mut encoder_prefix)
        .map_err(|e| MergeMineError::SerializeError(e.to_string()))?;
    coinbase
        .prefix
        .inputs
        .consensus_encode(&mut encoder_prefix)
        .map_err(|e| MergeMineError::SerializeError(e.to_string()))?;
    coinbase
        .prefix
        .outputs
        .consensus_encode(&mut encoder_prefix)
        .map_err(|e| MergeMineError::SerializeError(e.to_string()))?;
    let coinbase_prefix = coinbase_prefix_from(&encoder_prefix, coinbase_prefix_mode)?;

    let t_hash = monero::Hash::from_slice(tari_hash.as_slice());
    let aux_chain_merkle_proof = create_merkle_proof(&ordered_aux_chain_hashes, &t_hash).ok_or_else(|| {
        MergeMineError::ValidationError(
            "create_merkle_proof returned None, could not find tari hash in ordered aux chain hashes".to_string(),
        )
    })?;
    #[allow(clippy::cast_possible_truncation)]
    Ok(MoneroPowData {
        header: block.header,
        randomx_key: seed,
        transaction_count: hashes.len() as u16,
        merkle_root: root,
        coinbase_merkle_proof,
        coinbase_tx_extra: block.miner_tx.prefix.extra,
        coinbase_prefix,
        aux_chain_merkle_proof,
    })
}

/// Creates a hex encoded Monero blockhashing_blob that's used by the pow hash
pub fn create_blockhashing_blob_from_block(block: &monero::Block) -> Result<String, MergeMineError> {
    let tx_hashes = create_ordered_transaction_hashes_from_block(block);
    let root = tree_hash(&tx_hashes)?;
    let blob = create_block_hashing_blob(&block.header, &root, tx_hashes.len() as u64);
    Ok(hex::encode(blob))
}

/// Create a set of ordered transaction hashes from a Monero block
pub fn create_ordered_transaction_hashes_from_block(block: &monero::Block) -> Vec<monero::Hash> {
    iter::once(block.miner_tx.hash())
        .chain(block.tx_hashes.clone())
        .collect()
}

/// Inserts aux chain merkle root and info into a Monero block
pub fn insert_aux_chain_mr_and_info_into_block<T: AsRef<[u8]>>(
    block: &mut monero::Block,
    aux_chain_mr: T,
    aux_chain_count: u8,
    aux_nonce: u32,
) -> Result<(), MergeMineError> {
    if aux_chain_count == 0 {
        return Err(MergeMineError::ZeroAuxChains);
    }
    if aux_chain_mr.as_ref().len() != monero::Hash::len_bytes() {
        return Err(MergeMineError::HashingError(format!(
            "Expected source to be {} bytes, but it was {} bytes",
            monero::Hash::len_bytes(),
            aux_chain_mr.as_ref().len()
        )));
    }
    // When we insert the merge mining tag, we need to make sure that the extra field is valid.
    let mut extra_field = ExtraField::try_parse(&block.miner_tx.prefix.extra)
        .map_err(|_| MergeMineError::DeserializeError("Invalid extra field".to_string()))?;

    // Adding more than one merge mining tag is not allowed
    for item in &extra_field.0 {
        if let SubField::MergeMining(_, _) = item {
            return Err(MergeMineError::ValidationError(
                "More than one merge mining tag in coinbase not allowed".to_string(),
            ));
        }
    }

    // If `SubField::Padding(n)` with `n < 255` is the last sub field in the extra field, then appending a new field
    // will always fail deserialization (`ExtraField::try_parse`) - the new field cannot be parsed in that sequence.
    // To circumvent this, we create a new extra field by appending the original extra field to the merge mining field
    // instead.
    let hash = monero::Hash::from_slice(aux_chain_mr.as_ref());
    let encoded = if aux_chain_count == 1 {
        VarInt(0)
    } else {
        let mt_params = MerkleTreeParameters::new(aux_chain_count, aux_nonce)?;
        mt_params.to_varint()
    };
    extra_field.0.insert(0, SubField::MergeMining(encoded, hash));
    debug!(target: LOG_TARGET, "Inserted extra field: {extra_field:?}");

    block.miner_tx.prefix.extra = extra_field.into();

    // lets test the block to ensure its serializes correctly
    let blocktemplate_blob = serialize_monero_block_to_hex(block)?;
    let bytes = hex::decode(blocktemplate_blob).map_err(|_| HexError::HexConversionError {})?;
    let de_block = monero::consensus::deserialize::<monero::Block>(&bytes[..])
        .map_err(|_| MergeMineError::ValidationError("blocktemplate blob invalid".to_string()))?;
    if block != &de_block {
        return Err(MergeMineError::SerializeError(
            "Blocks dont match after serialization".to_string(),
        ));
    }
    Ok(())
}

/// Creates a hex encoded Monero blockhashing_blob
pub fn create_block_hashing_blob(
    header: &monero::BlockHeader,
    merkle_root: &monero::Hash,
    transaction_count: u64,
) -> Vec<u8> {
    let mut blockhashing_blob = consensus::serialize(header);
    blockhashing_blob.extend_from_slice(merkle_root.as_bytes());
    let mut count = consensus::serialize(&VarInt(transaction_count));
    blockhashing_blob.append(&mut count);
    blockhashing_blob
}

#[cfg(test)]
mod test {
    #![allow(clippy::indexing_slicing)]
    use std::{
        cmp::max,
        convert::{TryFrom, TryInto},
    };

    use borsh::{BorshDeserialize, BorshSerialize};
    use monero::{
        Hash,
        PublicKey,
        Transaction,
        TransactionPrefix,
        TxIn,
        TxOut,
        blockdata::transaction::TxOutTarget,
        consensus::deserialize,
        util::ringct::{RctSig, RctSigBase, RctType},
    };
    use serial_test::serial;
    use tari_common::configuration::Network;
    use tari_test_utils::unpack_enum;
    use tari_transaction_components::{
        consensus::consensus_constants::{
            ESMERALDA_DERIVED_MONERO_COINBASE_ACTIVATION_HEIGHT,
            MAINNET_STRICT_MERKLE_TREE_PARAMS_ACTIVATION_HEIGHT,
        },
        tari_proof_of_work::{PowAlgorithm, PowData, ProofOfWork},
    };
    use tari_utilities::{
        ByteArray,
        epoch_time::EpochTime,
        hex::{Hex, from_hex},
    };

    use super::*;
    use crate::proof_of_work::monero_rx::{
        merkle_tree::MerkleProof,
        merkle_tree_parameters::MerkleTreeParametersError,
    };

    // This tests checks the hash of monero-rs
    #[test]
    fn test_monero_rs_miner_tx_hash() {
        let tx = "f8ad7c58e6fce1792dd78d764ce88a11db0e3c3bb484d868ae05a7321fb6c6b0";

        let pk_extra = vec![
            179, 155, 220, 223, 213, 23, 81, 160, 95, 232, 87, 102, 151, 63, 70, 249, 139, 40, 110, 16, 51, 193, 175,
            208, 38, 120, 65, 191, 155, 139, 1, 4,
        ];
        let transaction = Transaction {
            prefix: TransactionPrefix {
                version: VarInt(2),
                unlock_time: VarInt(2143845),
                inputs: vec![TxIn::Gen {
                    height: VarInt(2143785),
                }],
                outputs: vec![TxOut {
                    amount: VarInt(1550800739964),
                    target: TxOutTarget::ToKey {
                        key: hex::decode("e2e19d8badb15e77c8e1f441cf6acd9bcde34a07cae82bbe5ff9629bf88e6e81")
                            .unwrap()
                            .as_slice()
                            .try_into()
                            .unwrap(),
                    },
                }],
                extra: ExtraField(vec![
                    SubField::TxPublicKey(PublicKey::from_slice(pk_extra.as_slice()).unwrap()),
                    SubField::Nonce(vec![196, 37, 4, 0, 27, 37, 187, 163, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
                ])
                .into(),
            },
            signatures: vec![],
            rct_signatures: RctSig {
                sig: Option::from(RctSigBase {
                    rct_type: RctType::Null,
                    txn_fee: Default::default(),
                    pseudo_outs: vec![],
                    ecdh_info: vec![],
                    out_pk: vec![],
                }),
                p: None,
            },
        };
        assert_eq!(
            tx.as_bytes().to_vec(),
            hex::encode(transaction.hash().0.to_vec()).as_bytes().to_vec()
        );
        let hex = hex::encode(consensus::serialize::<Transaction>(&transaction));
        deserialize::<Transaction>(&hex::decode(hex).unwrap()).unwrap();
    }

    // This tests checks the blockhashing blob of monero-rs
    #[test]
    fn test_monero_rs_block_serialize() {
        // block with only the miner tx and no other transactions
        let hex = "0c0c94debaf805beb3489c722a285c092a32e7c6893abfc7d069699c8326fc3445a749c5276b6200000000029b892201ffdf882201b699d4c8b1ec020223df524af2a2ef5f870adb6e1ceb03a475c39f8b9ef76aa50b46ddd2a18349402b012839bfa19b7524ec7488917714c216ca254b38ed0424ca65ae828a7c006aeaf10208f5316a7f6b99cca60000";
        // blockhashing blob for above block as accepted by monero
        let hex_blockhash_blob = "0c0c94debaf805beb3489c722a285c092a32e7c6893abfc7d069699c8326fc3445a749c5276b6200000000602d0d4710e2c2d38da0cce097accdf5dc18b1d34323880c1aae90ab8f6be6e201";
        let bytes = hex::decode(hex).unwrap();
        let block = deserialize::<monero::Block>(&bytes[..]).unwrap();
        let header = consensus::serialize::<monero::BlockHeader>(&block.header);
        let tx_count = 1 + block.tx_hashes.len() as u64;
        let mut count = consensus::serialize::<VarInt>(&VarInt(tx_count));
        #[allow(clippy::cast_possible_truncation)]
        let mut hashes = Vec::with_capacity(tx_count as usize);
        hashes.push(block.miner_tx.hash());
        for item in block.clone().tx_hashes {
            hashes.push(item);
        }
        let root = tree_hash(&hashes).unwrap();
        let mut encode2 = header;
        encode2.extend_from_slice(root.as_bytes());
        encode2.append(&mut count);
        assert_eq!(hex::encode(encode2), hex_blockhash_blob);
        let bytes2 = consensus::serialize::<monero::Block>(&block);
        assert_eq!(bytes, bytes2);
        let hex2 = hex::encode(bytes2);
        assert_eq!(hex, hex2);
    }

    #[test]
    fn test_monero_partial_hash() {
        let blocktemplate_blob = "0c0c8cd6a0fa057fe21d764e7abf004e975396a2160773b93712bf6118c3b4959ddd8ee0f76aad0000000002e1ea2701ffa5ea2701d5a299e2abb002028eb3066ced1b2cc82ea046f3716a48e9ae37144057d5fb48a97f941225a1957b2b0106225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa47820208f63aa86d2e857f070000".to_string();
        let bytes = hex::decode(blocktemplate_blob).unwrap();
        let mut block = deserialize::<monero::Block>(&bytes[..]).unwrap();
        let block_header = BlockHeader::new(0);
        let hash = block_header.merge_mining_hash();
        insert_aux_chain_mr_and_info_into_block(&mut block, hash, 1, 0).unwrap();

        let coinbase = block.miner_tx.clone();
        let extra = coinbase.prefix.extra;
        let mut keccak = Keccak::v256();
        let mut encoder_prefix = Vec::new();
        coinbase.prefix.version.consensus_encode(&mut encoder_prefix).unwrap();
        coinbase
            .prefix
            .unlock_time
            .consensus_encode(&mut encoder_prefix)
            .unwrap();
        coinbase.prefix.inputs.consensus_encode(&mut encoder_prefix).unwrap();
        coinbase.prefix.outputs.consensus_encode(&mut encoder_prefix).unwrap();
        keccak.update(&encoder_prefix);

        let mut finalised_prefix_keccak = keccak.clone();
        let mut encoder_extra_field = Vec::new();
        extra.consensus_encode(&mut encoder_extra_field).unwrap();
        finalised_prefix_keccak.update(&encoder_extra_field);
        let mut prefix_hash: [u8; 32] = [0; 32];
        finalised_prefix_keccak.finalize(&mut prefix_hash);

        let test_prefix_hash = block.miner_tx.prefix.hash();
        let test2 = monero::Hash::from_slice(&prefix_hash);
        assert_eq!(test_prefix_hash, test2);

        // let mut finalised_keccak = Keccak::v256();
        let rct_sig_base = RctSigBase {
            rct_type: RctType::Null,
            txn_fee: Default::default(),
            pseudo_outs: vec![],
            ecdh_info: vec![],
            out_pk: vec![],
        };
        let hashes = vec![test2, rct_sig_base.hash(), monero::Hash::null()];
        let encoder_final: Vec<u8> = hashes.into_iter().flat_map(|h| Vec::from(&h.to_bytes()[..])).collect();
        let coinbase = monero::Hash::new(encoder_final);
        let coinbase_hash = block.miner_tx.hash();
        assert_eq!(coinbase, coinbase_hash);
    }

    #[test]
    fn test_monero_data() {
        let blocktemplate_blob = "0c0c8cd6a0fa057fe21d764e7abf004e975396a2160773b93712bf6118c3b4959ddd8ee0f76aad0000000002e1ea2701ffa5ea2701d5a299e2abb002028eb3066ced1b2cc82ea046f3716a48e9ae37144057d5fb48a97f941225a1957b2b0106225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa47820208f63aa86d2e857f070000".to_string();
        let seed_hash = "9f02e032f9b15d2aded991e0f68cc3c3427270b568b782e55fbd269ead0bad97".to_string();
        let bytes = hex::decode(blocktemplate_blob).unwrap();
        let rules = BaseNodeConsensusManager::builder(Network::LocalNet).build().unwrap();
        let mut block = deserialize::<monero::Block>(&bytes[..]).unwrap();
        let mut block_header = BlockHeader {
            version: 0,
            height: 0,
            prev_hash: FixedHash::zero(),
            timestamp: EpochTime::now(),
            output_mr: FixedHash::zero(),
            block_output_mr: FixedHash::zero(),
            output_smt_size: 0,
            kernel_mr: FixedHash::zero(),
            kernel_mmr_size: 0,
            input_mr: FixedHash::zero(),
            total_kernel_offset: Default::default(),
            total_script_offset: Default::default(),
            nonce: 0,
            pow: ProofOfWork::default(),
            validator_node_mr: FixedHash::zero(),
            validator_node_size: 0,
        };
        let hash = block_header.merge_mining_hash();
        insert_aux_chain_mr_and_info_into_block(&mut block, hash, 1, 0).unwrap();
        let hashes = create_ordered_transaction_hashes_from_block(&block);
        assert_eq!(hashes.len(), block.tx_hashes.len() + 1);
        let root = tree_hash(&hashes).unwrap();
        let coinbase_merkle_proof = create_merkle_proof(&hashes, &hashes[0]).unwrap();
        let aux_hashes = vec![monero::Hash::from_slice(hash.as_ref())];
        let aux_chain_merkle_proof = create_merkle_proof(&aux_hashes, &aux_hashes[0]).unwrap();

        let coinbase = block.miner_tx.clone();
        let extra = coinbase.prefix.extra;
        let mut keccak = Keccak::v256();
        let mut encoder_prefix = Vec::new();
        coinbase.prefix.version.consensus_encode(&mut encoder_prefix).unwrap();
        coinbase
            .prefix
            .unlock_time
            .consensus_encode(&mut encoder_prefix)
            .unwrap();
        coinbase.prefix.inputs.consensus_encode(&mut encoder_prefix).unwrap();
        coinbase.prefix.outputs.consensus_encode(&mut encoder_prefix).unwrap();
        keccak.update(&encoder_prefix);

        let monero_data = MoneroPowData {
            header: block.header,
            randomx_key: FixedByteArray::from_canonical_bytes(&from_hex(&seed_hash).unwrap()).unwrap(),
            transaction_count: u16::try_from(hashes.len()).unwrap(),
            merkle_root: root,
            coinbase_merkle_proof,
            coinbase_prefix: CoinbasePrefix::Prefix(encoder_prefix.clone().try_into().unwrap()),
            coinbase_tx_extra: extra.clone(),
            aux_chain_merkle_proof,
        };
        let mut serialized = Vec::new();
        monero_data.serialize(&mut serialized).unwrap();
        let pow = ProofOfWork {
            pow_algo: PowAlgorithm::RandomXM,
            pow_data: PowData::try_from(serialized).unwrap(),
        };
        block_header.pow = pow;
        MoneroPowData::from_header(&block_header, &rules).unwrap();

        // lets test the hashesh
        let mut finalised_prefix_keccak = keccak.clone();
        let mut encoder_extra_field = Vec::new();
        extra.consensus_encode(&mut encoder_extra_field).unwrap();
        finalised_prefix_keccak.update(&encoder_extra_field);
        let mut prefix_hash: [u8; 32] = [0; 32];
        finalised_prefix_keccak.finalize(&mut prefix_hash);

        let test_prefix_hash = block.miner_tx.prefix.hash();
        let test2 = monero::Hash::from_slice(&prefix_hash);
        assert_eq!(test_prefix_hash, test2);

        // let mut finalised_keccak = Keccak::v256();
        let rct_sig_base = RctSigBase {
            rct_type: RctType::Null,
            txn_fee: Default::default(),
            pseudo_outs: vec![],
            ecdh_info: vec![],
            out_pk: vec![],
        };
        let hashes = vec![test2, rct_sig_base.hash(), monero::Hash::null()];
        let encoder_final: Vec<u8> = hashes.into_iter().flat_map(|h| Vec::from(&h.to_bytes()[..])).collect();
        let coinbase = monero::Hash::new(encoder_final);
        let coinbase_hash = block.miner_tx.hash();
        assert_eq!(coinbase, coinbase_hash);
    }

    /// Binding the aux-chain merkle proof's branch length to the chain count declared in the merge mining tag.
    ///
    /// Without the binding, `calculate_root` walks an attacker-chosen `branch.len()` levels while the position
    /// check derives its own depth from `number_of_chains`. Under the single-chain encoding the position check is
    /// vacuous, so one Monero proof of work can commit to a whole tree of distinct Tari headers at one height.
    mod aux_chain_depth_binding {
        use integer_encoding::VarIntWriter;
        use tari_transaction_components::consensus::consensus_constants::{
            ESMERALDA_AUX_CHAIN_DEPTH_BINDING_ACTIVATION_HEIGHT,
            MAINNET_AUX_CHAIN_DEPTH_BINDING_ACTIVATION_HEIGHT,
        };

        use super::*;
        use crate::proof_of_work::monero_rx::merkle_tree::MerkleProof;

        const BLOCKTEMPLATE_BLOB: &str = "0c0c8cd6a0fa057fe21d764e7abf004e975396a2160773b93712bf6118c3b4959ddd8ee0f76aad0000000002e1ea2701ffa5ea2701d5a299e2abb002028eb3066ced1b2cc82ea046f3716a48e9ae37144057d5fb48a97f941225a1957b2b0106225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa47820208f63aa86d2e857f070000";
        const SEED_HASH: &str = "9f02e032f9b15d2aded991e0f68cc3c3427270b568b782e55fbd269ead0bad97";

        /// `MoneroPowData` carrying the given aux-chain proof. `check_aux_chains` reads nothing else from it.
        fn pow_data_with(aux_chain_merkle_proof: MerkleProof) -> MoneroPowData {
            let coinbase: monero::Transaction = Default::default();
            let mut keccak = Keccak::v256();
            let mut encoder_prefix = Vec::new();
            coinbase.prefix.version.consensus_encode(&mut encoder_prefix).unwrap();
            coinbase
                .prefix
                .unlock_time
                .consensus_encode(&mut encoder_prefix)
                .unwrap();
            coinbase.prefix.inputs.consensus_encode(&mut encoder_prefix).unwrap();
            coinbase.prefix.outputs.consensus_encode(&mut encoder_prefix).unwrap();
            keccak.update(&encoder_prefix);

            MoneroPowData {
                header: Default::default(),
                randomx_key: FixedByteArray::default(),
                transaction_count: 1,
                merkle_root: Default::default(),
                coinbase_merkle_proof: MerkleProof::default(),
                // Fed straight to `check_aux_chains`, which never reads the coinbase prefix, so the legacy form
                // is fine here regardless of height.
                coinbase_prefix: CoinbasePrefix::Legacy(keccak),
                coinbase_tx_extra: coinbase.prefix.extra,
                aux_chain_merkle_proof,
            }
        }

        /// A proof built the way one actually arrives - through Borsh - rather than through a constructor. That
        /// keeps the forgeries honest: each one is something a peer could really put on the wire.
        fn wire_proof(branch: &[monero::Hash], path_bitmap: u32) -> MerkleProof {
            let mut buf = Vec::new();
            buf.write_varint(branch.len()).unwrap();
            for hash in branch {
                buf.extend_from_slice(hash.as_bytes());
            }
            buf.write_varint(path_bitmap).unwrap();
            MerkleProof::deserialize(&mut buf.as_slice()).unwrap()
        }

        /// Mirrors the leaf position derivation in `check_aux_chains`. It depends only on the genesis hash and the
        /// aux nonce - never on the proof - which is what lets a verifier know the expected branch length before
        /// it trusts anything the miner supplied.
        fn derived_position(genesis: &FixedHash, aux_nonce: u32, number_of_chains: u8) -> u32 {
            U256::from_little_endian(
                &Sha256::new()
                    .chain_update(genesis)
                    .chain_update(aux_nonce.to_le_bytes())
                    .chain_update((109_u8).to_le_bytes())
                    .finalize(),
            )
            .low_u32()
            .checked_rem(u32::from(number_of_chains))
            .expect("number_of_chains is never zero here")
        }

        /// The vulnerability, and its fix: a tag declaring one aux chain, carrying a real proof into a four leaf
        /// tree. Every one of the four Tari headers validates against the same Monero root before the fork.
        #[test]
        fn a_single_chain_tag_cannot_carry_a_tree_of_tari_headers() {
            let genesis = FixedHash::zero();
            let tari_hashes: Vec<FixedHash> = (0..4u8).map(|i| FixedHash::from([i; 32])).collect();
            let aux_hashes: Vec<monero::Hash> = tari_hashes
                .iter()
                .map(|h| monero::Hash::from_slice(h.as_slice()))
                .collect();
            let aux_root = tree_hash(&aux_hashes).unwrap();

            for (pos, tari_hash) in tari_hashes.iter().enumerate() {
                let proof = create_merkle_proof(&aux_hashes, &aux_hashes[pos]).unwrap();
                let monero_data = pow_data_with(proof);
                assert!(
                    check_aux_chains(&monero_data, VarInt(0), &aux_root, tari_hash, &genesis, false, false, 0).is_ok(),
                    "pre-fork rules should still accept header {pos}; this is the vulnerability being grandfathered"
                );
                assert!(
                    check_aux_chains(&monero_data, VarInt(0), &aux_root, tari_hash, &genesis, true, true, 0).is_err(),
                    "header {pos} is still accepted under a single-chain tag"
                );
            }
        }

        /// The honest single-chain case: the Tari hash *is* the root and the branch is empty.
        #[test]
        fn an_honest_single_chain_proof_is_accepted() {
            let tari_hash = FixedHash::from([9u8; 32]);
            let aux = monero::Hash::from_slice(tari_hash.as_slice());
            let proof = create_merkle_proof(&[aux], &aux).unwrap();
            assert!(proof.branch().is_empty());
            for enforce in [false, true] {
                assert!(
                    check_aux_chains(
                        &pow_data_with(proof.clone()),
                        VarInt(0),
                        &aux,
                        &tari_hash,
                        &FixedHash::zero(),
                        enforce,
                        enforce,
                        0
                    )
                    .is_ok(),
                    "enforce={enforce}"
                );
            }
        }

        /// Every branch length except the honest one is rejected, at the honest leaf position, for a range of
        /// chain counts. The path bitmap is left honest so the derived position is unchanged and the length is the
        /// only thing that varies.
        #[test]
        fn wrong_branch_lengths_are_rejected_for_every_chain_count() {
            let genesis = FixedHash::from([3u8; 32]);
            let aux_nonce = 7u32;
            let tari_hash = FixedHash::from([42u8; 32]);
            let t_hash = monero::Hash::from_slice(tari_hash.as_slice());

            for n in [2u8, 3, 4, 5, 255] {
                let params = MerkleTreeParameters::new(n, aux_nonce).unwrap();
                let varint = params.to_varint();
                assert_eq!(
                    MerkleTreeParameters::from_varint(varint.clone()),
                    Ok(params.clone()),
                    "n={n} varint round trip"
                );

                // Distinct filler leaves that cannot collide with `t_hash`: byte 1 is 0xAA, which [42; 32] is not.
                let mut aux_hashes: Vec<monero::Hash> = (0..n)
                    .map(|i| {
                        let mut buf = [0xAAu8; 32];
                        buf[0] = i;
                        monero::Hash::from_slice(&buf)
                    })
                    .collect();
                // Put Tari at the position the protocol assigns it, as an honest miner would.
                let pos = derived_position(&genesis, aux_nonce, n);
                aux_hashes[usize::try_from(pos).unwrap()] = t_hash;
                let aux_root = tree_hash(&aux_hashes).unwrap();

                let honest = create_merkle_proof(&aux_hashes, &t_hash).unwrap();
                let honest_len = honest.branch().len();
                assert_eq!(honest_len, expected_branch_len(n, pos), "n={n}");

                // The honest proof is accepted either side of the fork: the binding rejects forgeries only.
                for enforce in [false, true] {
                    assert!(
                        check_aux_chains(
                            &pow_data_with(honest.clone()),
                            varint.clone(),
                            &aux_root,
                            &tari_hash,
                            &genesis,
                            enforce,
                            enforce,
                            0
                        )
                        .is_ok(),
                        "n={n} honest proof rejected with enforce={enforce}"
                    );
                }

                for len in 0..(honest_len + 3).min(31) {
                    if len == honest_len {
                        continue;
                    }
                    let mut branch = honest.branch().to_vec();
                    branch.resize(len, monero::Hash::null());
                    let forged = wire_proof(&branch, honest.path());
                    assert!(
                        check_aux_chains(
                            &pow_data_with(forged),
                            varint.clone(),
                            &aux_root,
                            &tari_hash,
                            &genesis,
                            true,
                            true,
                            0
                        )
                        .is_err(),
                        "n={n}: branch of length {len} accepted, honest length is {honest_len}"
                    );
                }
            }
        }

        /// A full merge-mined block whose tag claims one aux chain while the proof reaches into a four leaf tree.
        /// Returns the header and the Tari merge mining hash it commits to.
        fn attack_block_at_height(rules: &BaseNodeConsensusManager, height: u64) -> BlockHeader {
            let bytes = hex::decode(BLOCKTEMPLATE_BLOB).unwrap();
            let mut block = deserialize::<monero::Block>(&bytes[..]).unwrap();
            let mut block_header = BlockHeader {
                version: 0,
                height,
                prev_hash: FixedHash::zero(),
                timestamp: EpochTime::now(),
                output_mr: FixedHash::zero(),
                block_output_mr: FixedHash::zero(),
                output_smt_size: 0,
                kernel_mr: FixedHash::zero(),
                kernel_mmr_size: 0,
                input_mr: FixedHash::zero(),
                total_kernel_offset: Default::default(),
                total_script_offset: Default::default(),
                nonce: 0,
                pow: ProofOfWork::default(),
                validator_node_mr: FixedHash::zero(),
                validator_node_size: 0,
            };
            let hash = block_header.merge_mining_hash();
            let t_hash = monero::Hash::from_slice(hash.as_ref());

            // Four candidate Tari headers under one aux root; ours is leaf 0.
            let mut aux_hashes = vec![t_hash];
            for i in 1..4u8 {
                aux_hashes.push(monero::Hash::from_slice(&[i; 32]));
            }
            let aux_root = tree_hash(&aux_hashes).unwrap();
            let aux_chain_merkle_proof = create_merkle_proof(&aux_hashes, &t_hash).unwrap();
            assert_eq!(aux_chain_merkle_proof.branch().len(), 2);

            // ...but the tag declares a single aux chain.
            insert_aux_chain_mr_and_info_into_block(&mut block, aux_root.as_bytes(), 1, 0).unwrap();

            // The coinbase changed, so the Monero side has to be recomputed after inserting the tag.
            let hashes = create_ordered_transaction_hashes_from_block(&block);
            let root = tree_hash(&hashes).unwrap();
            let coinbase_merkle_proof = create_merkle_proof(&hashes, &hashes[0]).unwrap();

            let coinbase = block.miner_tx.clone();
            let extra = coinbase.prefix.extra.clone();
            let mut encoder_prefix = Vec::new();
            coinbase.prefix.version.consensus_encode(&mut encoder_prefix).unwrap();
            coinbase
                .prefix
                .unlock_time
                .consensus_encode(&mut encoder_prefix)
                .unwrap();
            coinbase.prefix.inputs.consensus_encode(&mut encoder_prefix).unwrap();
            coinbase.prefix.outputs.consensus_encode(&mut encoder_prefix).unwrap();

            // GHSA-3qmx-q9pv-f3m4: which coinbase wire format is legal is a function of the height, so ask the
            // same question the node will ask rather than hard coding an answer.
            let coinbase_prefix = match CoinbasePrefixMode::for_height(rules, height) {
                CoinbasePrefixMode::Legacy => {
                    let mut keccak = Keccak::v256();
                    keccak.update(&encoder_prefix);
                    CoinbasePrefix::Legacy(keccak)
                },
                CoinbasePrefixMode::Derived => CoinbasePrefix::Prefix(encoder_prefix.try_into().unwrap()),
            };

            let monero_data = MoneroPowData {
                header: block.header,
                randomx_key: FixedByteArray::from_canonical_bytes(&from_hex(SEED_HASH).unwrap()).unwrap(),
                transaction_count: u16::try_from(hashes.len()).unwrap(),
                merkle_root: root,
                coinbase_merkle_proof,
                coinbase_prefix,
                coinbase_tx_extra: extra,
                aux_chain_merkle_proof,
            };
            let mut serialized = Vec::new();
            monero_data.serialize(&mut serialized).unwrap();
            block_header.pow = ProofOfWork {
                pow_algo: PowAlgorithm::RandomXM,
                pow_data: PowData::try_from(serialized).unwrap(),
            };
            block_header
        }

        /// The gate: the same forged block is accepted one block below the activation height and rejected at it.
        #[test]
        fn the_fix_is_gated_at_the_activation_height() {
            for (network, activation) in [
                (Network::Esmeralda, ESMERALDA_AUX_CHAIN_DEPTH_BINDING_ACTIVATION_HEIGHT),
                (Network::MainNet, MAINNET_AUX_CHAIN_DEPTH_BINDING_ACTIVATION_HEIGHT),
            ] {
                let rules = BaseNodeConsensusManager::builder(network).build().unwrap();

                let below = attack_block_at_height(&rules, activation.saturating_sub(1));
                verify_header(&below, &FixedHash::zero(), &rules).unwrap_or_else(|e| {
                    panic!("{network}: a pre-fork block must keep validating, but it failed with {e}")
                });

                let at = attack_block_at_height(&rules, activation);
                let err = verify_header(&at, &FixedHash::zero(), &rules).expect_err(&format!(
                    "{network}: the forgery is still accepted at the activation height"
                ));
                unpack_enum!(MergeMineError::ValidationError(details) = err);
                // The message must name the real fault. It used to say "Expected merge mining tag was not found",
                // which was false: the tag was found and parsed, its proof just did not check out.
                assert!(
                    details.contains("branch length does not match the chain count"),
                    "{network}: unexpected error {details}"
                );
                assert!(
                    !details.contains("Expected merge mining tag was not found"),
                    "{network}: the rejection still reports a missing tag: {details}"
                );
            }
        }

        /// An honest merge-mined block at `height`: one aux chain declared, one aux leaf, an empty branch - the
        /// shape the merge mining proxy actually produces. When `path_bitmap` is `Some`, the aux-chain proof's
        /// bitmap is overwritten with that value before serialization; with an empty branch every one of its 32
        /// bits is unread, so this is exactly the malleability under test. Returns the header and the Tari merge
        /// mining hash it commits to.
        fn honest_block_at_height(
            rules: &BaseNodeConsensusManager,
            height: u64,
            path_bitmap: Option<u32>,
        ) -> (BlockHeader, FixedHash) {
            let bytes = hex::decode(BLOCKTEMPLATE_BLOB).unwrap();
            let mut block = deserialize::<monero::Block>(&bytes[..]).unwrap();
            let mut block_header = BlockHeader {
                version: 0,
                height,
                prev_hash: FixedHash::zero(),
                // Fixed, not `now()`: two calls at the same height must differ in nothing but the path bitmap,
                // otherwise the malleability comparison below proves nothing.
                timestamp: EpochTime::from(1_700_000_000u64),
                output_mr: FixedHash::zero(),
                block_output_mr: FixedHash::zero(),
                output_smt_size: 0,
                kernel_mr: FixedHash::zero(),
                kernel_mmr_size: 0,
                input_mr: FixedHash::zero(),
                total_kernel_offset: Default::default(),
                total_script_offset: Default::default(),
                nonce: 0,
                pow: ProofOfWork::default(),
                validator_node_mr: FixedHash::zero(),
                validator_node_size: 0,
            };
            let hash = block_header.merge_mining_hash();
            insert_aux_chain_mr_and_info_into_block(&mut block, hash, 1, 0).unwrap();

            let hashes = create_ordered_transaction_hashes_from_block(&block);
            let root = tree_hash(&hashes).unwrap();
            let coinbase_merkle_proof = create_merkle_proof(&hashes, &hashes[0]).unwrap();
            let aux_hashes = vec![monero::Hash::from_slice(hash.as_ref())];
            let aux_chain_merkle_proof = match path_bitmap {
                // Round-tripped through Borsh, so the tampered proof is something a peer could really put on the
                // wire rather than something only a constructor can build.
                Some(bitmap) => wire_proof(&[], bitmap),
                None => create_merkle_proof(&aux_hashes, &aux_hashes[0]).unwrap(),
            };
            assert!(aux_chain_merkle_proof.branch().is_empty());

            let coinbase = block.miner_tx.clone();
            let extra = coinbase.prefix.extra.clone();
            let mut encoder_prefix = Vec::new();
            coinbase.prefix.version.consensus_encode(&mut encoder_prefix).unwrap();
            coinbase
                .prefix
                .unlock_time
                .consensus_encode(&mut encoder_prefix)
                .unwrap();
            coinbase.prefix.inputs.consensus_encode(&mut encoder_prefix).unwrap();
            coinbase.prefix.outputs.consensus_encode(&mut encoder_prefix).unwrap();

            // GHSA-3qmx-q9pv-f3m4: which coinbase wire format is legal is a function of the height, so ask the
            // same question the node will ask rather than hard coding an answer.
            let coinbase_prefix = match CoinbasePrefixMode::for_height(rules, height) {
                CoinbasePrefixMode::Legacy => {
                    let mut keccak = Keccak::v256();
                    keccak.update(&encoder_prefix);
                    CoinbasePrefix::Legacy(keccak)
                },
                CoinbasePrefixMode::Derived => CoinbasePrefix::Prefix(encoder_prefix.try_into().unwrap()),
            };

            let monero_data = MoneroPowData {
                header: block.header,
                randomx_key: FixedByteArray::from_canonical_bytes(&from_hex(SEED_HASH).unwrap()).unwrap(),
                transaction_count: u16::try_from(hashes.len()).unwrap(),
                merkle_root: root,
                coinbase_merkle_proof,
                coinbase_prefix,
                coinbase_tx_extra: extra,
                aux_chain_merkle_proof,
            };
            let mut serialized = Vec::new();
            monero_data.serialize(&mut serialized).unwrap();
            block_header.pow = ProofOfWork {
                pow_algo: PowAlgorithm::RandomXM,
                pow_data: PowData::try_from(serialized).unwrap(),
            };

            (block_header, hash)
        }

        /// Regression: an honest merge-mined block validates exactly as before, with the binding live.
        #[test]
        fn an_honest_merge_mined_block_still_validates() {
            let rules = BaseNodeConsensusManager::builder(Network::LocalNet).build().unwrap();
            assert!(
                rules.consensus_constants(0).aux_chain_merkle_proof_depth_binding(),
                "LocalNet is expected to have the binding live from height 0"
            );

            let (block_header, hash) = honest_block_at_height(&rules, 0, None);
            verify_header(&block_header, &hash, &rules).unwrap();
        }

        /// The malleability the bitmap rule closes. In production `aux_chain_count == 1`, so the branch is empty,
        /// `calculate_root` returns early and `get_position_from_path` never reads the bitmap: all 32 bits are
        /// free. Flipping one changes the serialized `pow_data`, and therefore `BlockHeader::hash()` - which
        /// chains `pow` where `merge_mining_hash()` does not - while leaving the proof of work, the root and the
        /// position identical. That is a distinct, still valid block hash for no hashpower, which misses the
        /// block-hash-keyed dedup, bad block cache and reconciliation lock and buys a free RandomX hash off every
        /// node that receives it.
        #[test]
        fn a_flipped_unread_path_bit_is_rejected_only_after_the_fork() {
            for (network, activation) in [
                (Network::Esmeralda, ESMERALDA_AUX_CHAIN_DEPTH_BINDING_ACTIVATION_HEIGHT),
                (Network::MainNet, MAINNET_AUX_CHAIN_DEPTH_BINDING_ACTIVATION_HEIGHT),
            ] {
                let rules = BaseNodeConsensusManager::builder(network).build().unwrap();

                for bit in [0u32, 1, 7, 8, 16, 31] {
                    let bitmap = 1u32 << bit;

                    // The tampering really does produce a different block off the same proof of work.
                    let (honest, _) = honest_block_at_height(&rules, activation, None);
                    let (tampered, hash) = honest_block_at_height(&rules, activation, Some(bitmap));
                    assert_eq!(
                        honest.merge_mining_hash(),
                        tampered.merge_mining_hash(),
                        "{network} bit {bit}: the merge mining hash must be untouched, or this proves nothing"
                    );
                    assert_ne!(
                        honest.hash(),
                        tampered.hash(),
                        "{network} bit {bit}: the block hash must differ, or there is no malleability"
                    );

                    let (below, below_hash) =
                        honest_block_at_height(&rules, activation.saturating_sub(1), Some(bitmap));
                    verify_header(&below, &below_hash, &rules).unwrap_or_else(|e| {
                        panic!("{network} bit {bit}: a pre-fork block must keep validating, but it failed with {e}")
                    });

                    let err = verify_header(&tampered, &hash, &rules).expect_err(&format!(
                        "{network} bit {bit}: the flipped path bit is still accepted at the activation height"
                    ));
                    unpack_enum!(MergeMineError::ValidationError(details) = err);
                    assert!(
                        details.contains("path bitmap"),
                        "{network} bit {bit}: unexpected error {details}"
                    );
                }
            }
        }

        /// The refactor from `bool` to `Result` must move the *message*, never the verdict. Every branch of
        /// `check_aux_chains` is pinned here on both sides of the fork; if a future edit to the error plumbing
        /// changes an accept into a reject or vice versa, this fails.
        #[test]
        fn the_accept_reject_verdict_is_pinned_on_both_sides_of_the_fork() {
            let genesis = FixedHash::from([3u8; 32]);
            let aux_nonce = 7u32;
            let tari_hash = FixedHash::from([42u8; 32]);
            let t_hash = monero::Hash::from_slice(tari_hash.as_slice());
            let other_root = monero::Hash::from_slice(&[0x5Au8; 32]);

            // (case name, merge mining params, proof, aux root, accepted pre-fork, accepted post-fork)
            let mut cases: Vec<(&str, VarInt, MerkleProof, monero::Hash, bool, bool)> = vec![
                ("honest single chain", VarInt(0), wire_proof(&[], 0), t_hash, true, true),
                (
                    "single chain, non-empty branch",
                    VarInt(0),
                    wire_proof(&[monero::Hash::null()], 0),
                    t_hash,
                    true,
                    false,
                ),
                (
                    "single chain, unread path bit",
                    VarInt(0),
                    wire_proof(&[], 1 << 31),
                    t_hash,
                    true,
                    false,
                ),
                (
                    "single chain, wrong root",
                    VarInt(0),
                    wire_proof(&[], 0),
                    other_root,
                    false,
                    false,
                ),
            ];

            // A real four-leaf tree, with Tari at the position the protocol assigns it.
            let params = MerkleTreeParameters::new(4, aux_nonce).unwrap();
            let varint = params.to_varint();
            let pos = derived_position(&genesis, aux_nonce, 4);
            let mut aux_hashes: Vec<monero::Hash> = (0..4u8)
                .map(|i| {
                    let mut buf = [0xAAu8; 32];
                    buf[0] = i;
                    monero::Hash::from_slice(&buf)
                })
                .collect();
            aux_hashes[usize::try_from(pos).unwrap()] = t_hash;
            let aux_root = tree_hash(&aux_hashes).unwrap();
            let honest = create_merkle_proof(&aux_hashes, &t_hash).unwrap();
            assert_eq!(honest.branch().len(), 2);
            cases.push((
                "four chains, honest",
                varint.clone(),
                honest.clone(),
                aux_root,
                true,
                true,
            ));
            cases.push((
                "four chains, unread path bit",
                varint.clone(),
                wire_proof(honest.branch(), honest.path() | (1 << 20)),
                aux_root,
                true,
                false,
            ));
            cases.push((
                "four chains, short branch",
                varint.clone(),
                wire_proof(&honest.branch()[..1], honest.path()),
                aux_root,
                false,
                false,
            ));
            cases.push((
                "four chains, long branch",
                varint.clone(),
                wire_proof(&[honest.branch(), &[monero::Hash::null()]].concat(), honest.path()),
                aux_root,
                false,
                false,
            ));
            cases.push(("four chains, wrong root", varint, honest, other_root, false, false));

            for (name, params, proof, root, pre_fork, post_fork) in cases {
                for (enforce, expected) in [(false, pre_fork), (true, post_fork)] {
                    let verdict = check_aux_chains(
                        &pow_data_with(proof.clone()),
                        params.clone(),
                        &root,
                        &tari_hash,
                        &genesis,
                        enforce,
                        enforce,
                        0,
                    )
                    .is_ok();
                    assert_eq!(
                        verdict, expected,
                        "{name}: with depth binding {enforce} the verdict must be {expected}"
                    );
                }
            }
        }
    }

    #[test]
    fn test_input_blob() {
        let blocktemplate_blob = "0c0c8cd6a0fa057fe21d764e7abf004e975396a2160773b93712bf6118c3b4959ddd8ee0f76aad0000000002e1ea2701ffa5ea2701d5a299e2abb002028eb3066ced1b2cc82ea046f3716a48e9ae37144057d5fb48a97f941225a1957b2b0106225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa47820208f63aa86d2e857f070000".to_string();
        let bytes = hex::decode(blocktemplate_blob).unwrap();
        let block = deserialize::<monero::Block>(&bytes[..]).unwrap();
        let input_blob = create_blockhashing_blob_from_block(&block).unwrap();
        assert_eq!(
            input_blob,
            "0c0c8cd6a0fa057fe21d764e7abf004e975396a2160773b93712bf6118c3b4959ddd8ee0f76aad0000000058b030b6800d433bbcb2b560afe2a08e4dc152fa77ead96d37aaf14897d3c09601"
        );
    }

    #[test]
    fn test_append_mm_tag() {
        let rules = BaseNodeConsensusManager::builder(Network::LocalNet).build().unwrap();
        let blocktemplate_blob = "0c0c8cd6a0fa057fe21d764e7abf004e975396a2160773b93712bf6118c3b4959ddd8ee0f76aad0000000002e1ea2701ffa5ea2701d5a299e2abb002028eb3066ced1b2cc82ea046f3716a48e9ae37144057d5fb48a97f941225a1957b2b0106225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa47820208f63aa86d2e857f070000".to_string();
        let seed_hash = "9f02e032f9b15d2aded991e0f68cc3c3427270b568b782e55fbd269ead0bad97".to_string();
        let bytes = hex::decode(blocktemplate_blob).unwrap();
        let mut block = deserialize::<monero::Block>(&bytes[..]).unwrap();
        let mut block_header = BlockHeader {
            version: 0,
            height: 0,
            prev_hash: FixedHash::zero(),
            timestamp: EpochTime::now(),
            output_mr: FixedHash::zero(),
            block_output_mr: FixedHash::zero(),
            output_smt_size: 0,
            kernel_mr: FixedHash::zero(),
            kernel_mmr_size: 0,
            input_mr: FixedHash::zero(),
            total_kernel_offset: Default::default(),
            total_script_offset: Default::default(),
            nonce: 0,
            pow: ProofOfWork::default(),
            validator_node_mr: FixedHash::zero(),
            validator_node_size: 0,
        };
        let hash = block_header.merge_mining_hash();
        insert_aux_chain_mr_and_info_into_block(&mut block, hash, 1, 0).unwrap();
        let count = 1 + (u16::try_from(block.tx_hashes.len()).unwrap());
        let mut hashes = Vec::with_capacity(count as usize);
        hashes.push(block.miner_tx.hash());
        // Note: tx_hashes is empty, so |hashes| == 1
        for item in block.clone().tx_hashes {
            hashes.push(item);
        }
        let root = tree_hash(&hashes).unwrap();
        assert_eq!(root, hashes[0]);
        let coinbase_merkle_proof = create_merkle_proof(&hashes, &hashes[0]).unwrap();
        let aux_hashes = vec![monero::Hash::from_slice(hash.as_ref())];
        let aux_chain_merkle_proof = create_merkle_proof(&aux_hashes, &aux_hashes[0]).unwrap();

        let coinbase = block.miner_tx.clone();
        let extra = coinbase.prefix.extra.clone();
        let mut keccak = Keccak::v256();
        let mut encoder_prefix = Vec::new();
        coinbase.prefix.version.consensus_encode(&mut encoder_prefix).unwrap();
        coinbase
            .prefix
            .unlock_time
            .consensus_encode(&mut encoder_prefix)
            .unwrap();
        coinbase.prefix.inputs.consensus_encode(&mut encoder_prefix).unwrap();
        coinbase.prefix.outputs.consensus_encode(&mut encoder_prefix).unwrap();
        keccak.update(&encoder_prefix);

        let monero_data = MoneroPowData {
            header: block.header,
            randomx_key: FixedByteArray::from_canonical_bytes(&from_hex(&seed_hash).unwrap()).unwrap(),
            transaction_count: count,
            merkle_root: root,
            coinbase_merkle_proof,
            coinbase_prefix: CoinbasePrefix::Prefix(encoder_prefix.clone().try_into().unwrap()),
            coinbase_tx_extra: extra,
            aux_chain_merkle_proof,
        };
        let mut serialized = Vec::new();
        monero_data.serialize(&mut serialized).unwrap();
        let pow = ProofOfWork {
            pow_algo: PowAlgorithm::RandomXM,
            pow_data: PowData::try_from(serialized).unwrap(),
        };
        block_header.pow = pow;

        verify_header(&block_header, &hash, &rules).unwrap();
    }

    #[test]
    fn test_append_mm_tag_no_tag() {
        let rules = BaseNodeConsensusManager::builder(Network::LocalNet).build().unwrap();
        let blocktemplate_blob = "0c0c8cd6a0fa057fe21d764e7abf004e975396a2160773b93712bf6118c3b4959ddd8ee0f76aad0000000002e1ea2701ffa5ea2701d5a299e2abb002028eb3066ced1b2cc82ea046f3716a48e9ae37144057d5fb48a97f941225a1957b2b0106225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa47820208f63aa86d2e857f070000".to_string();
        let seed_hash = "9f02e032f9b15d2aded991e0f68cc3c3427270b568b782e55fbd269ead0bad97".to_string();
        let bytes = hex::decode(blocktemplate_blob).unwrap();
        let block = deserialize::<monero::Block>(&bytes[..]).unwrap();
        let mut block_header = BlockHeader {
            version: 0,
            height: 0,
            prev_hash: FixedHash::zero(),
            timestamp: EpochTime::now(),
            output_mr: FixedHash::zero(),
            block_output_mr: FixedHash::zero(),
            output_smt_size: 0,
            kernel_mr: FixedHash::zero(),
            kernel_mmr_size: 0,
            input_mr: FixedHash::zero(),
            total_kernel_offset: Default::default(),
            total_script_offset: Default::default(),
            nonce: 0,
            pow: ProofOfWork::default(),
            validator_node_mr: FixedHash::zero(),
            validator_node_size: 0,
        };
        let count = 1 + (u16::try_from(block.tx_hashes.len()).unwrap());
        let mut hashes = Vec::with_capacity(count as usize);
        hashes.push(block.miner_tx.hash());
        for item in block.clone().tx_hashes {
            hashes.push(item);
        }
        let root = tree_hash(&hashes).unwrap();
        let coinbase_merkle_proof = create_merkle_proof(&hashes, &hashes[0]).unwrap();
        let aux_hashes = vec![monero::Hash::from_slice(block_header.hash().as_ref())];
        let aux_chain_merkle_proof = create_merkle_proof(&aux_hashes, &aux_hashes[0]).unwrap();

        let coinbase = block.miner_tx.clone();
        let extra = coinbase.prefix.extra.clone();
        let mut keccak = Keccak::v256();
        let mut encoder_prefix = Vec::new();
        coinbase.prefix.version.consensus_encode(&mut encoder_prefix).unwrap();
        coinbase
            .prefix
            .unlock_time
            .consensus_encode(&mut encoder_prefix)
            .unwrap();
        coinbase.prefix.inputs.consensus_encode(&mut encoder_prefix).unwrap();
        coinbase.prefix.outputs.consensus_encode(&mut encoder_prefix).unwrap();
        keccak.update(&encoder_prefix);

        let monero_data = MoneroPowData {
            header: block.header,
            randomx_key: FixedByteArray::from_canonical_bytes(&from_hex(&seed_hash).unwrap()).unwrap(),
            transaction_count: count,
            merkle_root: root,
            coinbase_merkle_proof,
            coinbase_prefix: CoinbasePrefix::Prefix(encoder_prefix.clone().try_into().unwrap()),
            coinbase_tx_extra: extra,
            aux_chain_merkle_proof,
        };

        let mut serialized = Vec::new();
        monero_data.serialize(&mut serialized).unwrap();
        let pow = ProofOfWork {
            pow_algo: PowAlgorithm::RandomXM,
            pow_data: PowData::try_from(serialized).unwrap(),
        };
        block_header.pow = pow;
        let err = verify_header(&block_header, &block_header.hash(), &rules).unwrap_err();
        unpack_enum!(MergeMineError::ValidationError(details) = err);
        assert!(details.contains("Expected merge mining tag was not found in Monero coinbase transaction"));
    }

    #[test]
    fn test_append_mm_tag_wrong_hash() {
        let rules = BaseNodeConsensusManager::builder(Network::LocalNet).build().unwrap();
        let blocktemplate_blob = "0c0c8cd6a0fa057fe21d764e7abf004e975396a2160773b93712bf6118c3b4959ddd8ee0f76aad0000000002e1ea2701ffa5ea2701d5a299e2abb002028eb3066ced1b2cc82ea046f3716a48e9ae37144057d5fb48a97f941225a1957b2b0106225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa47820208f63aa86d2e857f070000".to_string();
        let seed_hash = "9f02e032f9b15d2aded991e0f68cc3c3427270b568b782e55fbd269ead0bad97".to_string();
        let bytes = hex::decode(blocktemplate_blob).unwrap();
        let mut block = deserialize::<monero::Block>(&bytes[..]).unwrap();
        let mut block_header = BlockHeader {
            version: 0,
            height: 0,
            prev_hash: FixedHash::zero(),
            timestamp: EpochTime::now(),
            output_mr: FixedHash::zero(),
            block_output_mr: FixedHash::zero(),
            output_smt_size: 0,
            kernel_mr: FixedHash::zero(),
            kernel_mmr_size: 0,
            input_mr: FixedHash::zero(),
            total_kernel_offset: Default::default(),
            total_script_offset: Default::default(),
            nonce: 0,
            pow: ProofOfWork::default(),
            validator_node_mr: FixedHash::zero(),
            validator_node_size: 0,
        };
        let hash = Hash::null();
        insert_aux_chain_mr_and_info_into_block(&mut block, hash, 1, 0).unwrap();
        let count = 1 + (u16::try_from(block.tx_hashes.len()).unwrap());
        let mut hashes = Vec::with_capacity(count as usize);
        let mut proof = Vec::with_capacity(count as usize);
        hashes.push(block.miner_tx.hash());
        proof.push(block.miner_tx.hash());
        for item in block.clone().tx_hashes {
            hashes.push(item);
            proof.push(item);
        }
        let root = tree_hash(&hashes).unwrap();
        let coinbase_merkle_proof = create_merkle_proof(&hashes, &hashes[0]).unwrap();
        let aux_hashes = vec![monero::Hash::from_slice(hash.as_ref())];
        let aux_chain_merkle_proof = create_merkle_proof(&aux_hashes, &aux_hashes[0]).unwrap();

        let coinbase = block.miner_tx.clone();
        let extra = coinbase.prefix.extra.clone();
        let mut keccak = Keccak::v256();
        let mut encoder_prefix = Vec::new();
        coinbase.prefix.version.consensus_encode(&mut encoder_prefix).unwrap();
        coinbase
            .prefix
            .unlock_time
            .consensus_encode(&mut encoder_prefix)
            .unwrap();
        coinbase.prefix.inputs.consensus_encode(&mut encoder_prefix).unwrap();
        coinbase.prefix.outputs.consensus_encode(&mut encoder_prefix).unwrap();
        keccak.update(&encoder_prefix);

        let monero_data = MoneroPowData {
            header: block.header,
            randomx_key: FixedByteArray::from_canonical_bytes(&from_hex(&seed_hash).unwrap()).unwrap(),
            transaction_count: count,
            merkle_root: root,
            coinbase_merkle_proof,
            coinbase_prefix: CoinbasePrefix::Prefix(encoder_prefix.clone().try_into().unwrap()),
            coinbase_tx_extra: extra,
            aux_chain_merkle_proof,
        };
        let mut serialized = Vec::new();
        monero_data.serialize(&mut serialized).unwrap();
        let pow = ProofOfWork {
            pow_algo: PowAlgorithm::RandomXM,
            pow_data: PowData::try_from(serialized).unwrap(),
        };
        block_header.pow = pow;
        let err = verify_header(&block_header, &block_header.hash(), &rules).unwrap_err();
        unpack_enum!(MergeMineError::ValidationError(details) = err);
        // The tag *is* present and parses; it just commits to the wrong aux-chain root. The message used to claim
        // the tag was missing, which sent operators looking in the wrong place.
        assert!(
            details.contains("the proof reconstructs root"),
            "unexpected error {details}"
        );
        assert!(
            !details.contains("Expected merge mining tag was not found"),
            "a present-but-wrong tag is still reported as a missing one: {details}"
        );
    }

    #[test]
    fn test_duplicate_append_mm_tag() {
        let rules = BaseNodeConsensusManager::builder(Network::LocalNet).build().unwrap();
        let blocktemplate_blob = "0c0c8cd6a0fa057fe21d764e7abf004e975396a2160773b93712bf6118c3b4959ddd8ee0f76aad0000000002e1ea2701ffa5ea2701d5a299e2abb002028eb3066ced1b2cc82ea046f3716a48e9ae37144057d5fb48a97f941225a1957b2b0106225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa47820208f63aa86d2e857f070000".to_string();
        let seed_hash = "9f02e032f9b15d2aded991e0f68cc3c3427270b568b782e55fbd269ead0bad97".to_string();
        let bytes = hex::decode(blocktemplate_blob).unwrap();
        let mut block = deserialize::<monero::Block>(&bytes[..]).unwrap();
        let mut block_header = BlockHeader {
            version: 0,
            height: 0,
            prev_hash: FixedHash::zero(),
            timestamp: EpochTime::now(),
            output_mr: FixedHash::zero(),
            block_output_mr: FixedHash::zero(),
            output_smt_size: 0,
            kernel_mr: FixedHash::zero(),
            kernel_mmr_size: 0,
            input_mr: FixedHash::zero(),
            total_kernel_offset: Default::default(),
            total_script_offset: Default::default(),
            nonce: 0,
            pow: ProofOfWork::default(),
            validator_node_mr: FixedHash::zero(),
            validator_node_size: 0,
        };
        let hash = block_header.merge_mining_hash();
        insert_aux_chain_mr_and_info_into_block(&mut block, hash, 1, 0).unwrap();
        #[allow(clippy::redundant_clone)]
        let mut block_header2 = block_header.clone();
        block_header2.version = 1;
        let hash2 = block_header2.merge_mining_hash();
        assert!(extract_aux_merkle_root_from_block(&block).is_ok());

        // Try via the API - this will fail because more than one merge mining tag is not allowed
        assert!(insert_aux_chain_mr_and_info_into_block(&mut block, hash2, 1, 0).is_err());

        // Now bypass the API - this will effectively allow us to insert more than one merge mining tag,
        // like trying to sneek it in. Later on, when we call `verify_header(&block_header)`, it should fail.
        let mut extra_field = ExtraField::try_parse(&block.miner_tx.prefix.extra).unwrap();
        let hash = monero::Hash::from_slice(hash.as_ref());
        extra_field.0.insert(0, SubField::MergeMining(VarInt(0), hash));
        block.miner_tx.prefix.extra = extra_field.into();

        // Trying to extract the Tari hash will fail because there are more than one merge mining tag
        let err = extract_aux_merkle_root_from_block(&block).unwrap_err();
        unpack_enum!(MergeMineError::ValidationError(details) = err);
        assert!(details.contains("More than one merge mining tag found in coinbase"));

        let count = 1 + (u16::try_from(block.tx_hashes.len()).unwrap());
        let mut hashes = Vec::with_capacity(count as usize);
        hashes.push(block.miner_tx.hash());
        // Note: tx_hashes is empty, so |hashes| == 1
        for item in block.clone().tx_hashes {
            hashes.push(item);
        }
        let root = tree_hash(&hashes).unwrap();
        assert_eq!(root, hashes[0]);
        let coinbase_merkle_proof = create_merkle_proof(&hashes, &hashes[0]).unwrap();
        let aux_hashes = vec![monero::Hash::from_slice(hash.as_ref())];
        let aux_chain_merkle_proof = create_merkle_proof(&aux_hashes, &aux_hashes[0]).unwrap();

        let coinbase = block.miner_tx.clone();
        let extra = coinbase.prefix.extra.clone();
        let mut keccak = Keccak::v256();
        let mut encoder_prefix = Vec::new();
        coinbase.prefix.version.consensus_encode(&mut encoder_prefix).unwrap();
        coinbase
            .prefix
            .unlock_time
            .consensus_encode(&mut encoder_prefix)
            .unwrap();
        coinbase.prefix.inputs.consensus_encode(&mut encoder_prefix).unwrap();
        coinbase.prefix.outputs.consensus_encode(&mut encoder_prefix).unwrap();
        keccak.update(&encoder_prefix);

        let monero_data = MoneroPowData {
            header: block.header,
            randomx_key: FixedByteArray::from_canonical_bytes(&from_hex(&seed_hash).unwrap()).unwrap(),
            transaction_count: count,
            merkle_root: root,
            coinbase_merkle_proof,
            coinbase_prefix: CoinbasePrefix::Prefix(encoder_prefix.clone().try_into().unwrap()),
            coinbase_tx_extra: extra,
            aux_chain_merkle_proof,
        };
        let mut serialized = Vec::new();
        monero_data.serialize(&mut serialized).unwrap();
        let pow = ProofOfWork {
            pow_algo: PowAlgorithm::RandomXM,
            pow_data: PowData::try_from(serialized).unwrap(),
        };
        block_header.pow = pow;

        // Header verification will fail because there are more than one merge mining tag
        let err = verify_header(&block_header, &block_header.hash(), &rules).unwrap_err();
        unpack_enum!(MergeMineError::ValidationError(details) = err);
        assert!(details.contains("More than one merge mining tag found in coinbase"));
    }

    #[test]
    fn test_extra_field_with_parsing_error() {
        let blocktemplate_blob = "0c0c8cd6a0fa057fe21d764e7abf004e975396a2160773b93712bf6118c3b4959ddd8ee0f76aad0000000002e1ea2701ffa5ea2701d5a299e2abb002028eb3066ced1b2cc82ea046f3716a48e9ae37144057d5fb48a97f941225a1957b2b0106225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa47820208f63aa86d2e857f070000".to_string();
        let bytes = hex::decode(blocktemplate_blob).unwrap();
        let mut block = deserialize::<monero::Block>(&bytes[..]).unwrap();
        let block_header = BlockHeader {
            version: 0,
            height: 0,
            prev_hash: FixedHash::zero(),
            timestamp: EpochTime::now(),
            output_mr: FixedHash::zero(),
            block_output_mr: FixedHash::zero(),
            output_smt_size: 0,
            kernel_mr: FixedHash::zero(),
            kernel_mmr_size: 0,
            input_mr: FixedHash::zero(),
            total_kernel_offset: Default::default(),
            total_script_offset: Default::default(),
            nonce: 0,
            pow: ProofOfWork::default(),
            validator_node_mr: FixedHash::zero(),
            validator_node_size: 2,
        };

        // Let us manipulate the extra field to make it invalid
        let mut extra_field_before_parse = ExtraField::try_parse(&block.miner_tx.prefix.extra).unwrap();
        assert_eq!(
            "ExtraField([TxPublicKey(06225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa4782), Nonce([246, \
             58, 168, 109, 46, 133, 127, 7])])",
            &format!("{extra_field_before_parse:?}")
        );
        assert!(ExtraField::try_parse(&extra_field_before_parse.clone().into()).is_ok());

        extra_field_before_parse.0.insert(0, SubField::Padding(230));
        assert_eq!(
            "ExtraField([Padding(230), TxPublicKey(06225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa4782), \
             Nonce([246, 58, 168, 109, 46, 133, 127, 7])])",
            &format!("{extra_field_before_parse:?}")
        );
        assert!(ExtraField::try_parse(&extra_field_before_parse.clone().into()).is_err());

        // Now insert the merge mining tag - this would also clean up the extra field and remove the invalid sub-fields
        let hash = block_header.merge_mining_hash();
        insert_aux_chain_mr_and_info_into_block(&mut block, hash, 1, 0).unwrap();
        assert!(ExtraField::try_parse(&block.miner_tx.prefix.extra.clone()).is_ok());

        // Verify that the merge mining tag is there
        let extra_field_after_tag = ExtraField::try_parse(&block.miner_tx.prefix.extra.clone()).unwrap();
        assert_eq!(
            &format!(
                "ExtraField([MergeMining(0, 0x{}), \
                 TxPublicKey(06225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa4782), Nonce([246, 58, 168, \
                 109, 46, 133, 127, 7])])",
                hex::encode(hash)
            ),
            &format!("{extra_field_after_tag:?}")
        );
    }

    #[test]
    fn test_verify_header_no_coinbase() {
        let rules = BaseNodeConsensusManager::builder(Network::LocalNet).build().unwrap();
        let blocktemplate_blob = "0c0c8cd6a0fa057fe21d764e7abf004e975396a2160773b93712bf6118c3b4959ddd8ee0f76aad0000000002e1ea2701ffa5ea2701d5a299e2abb002028eb3066ced1b2cc82ea046f3716a48e9ae37144057d5fb48a97f941225a1957b2b0106225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa47820208f63aa86d2e857f070000".to_string();
        let seed_hash = "9f02e032f9b15d2aded991e0f68cc3c3427270b568b782e55fbd269ead0bad97".to_string();
        let bytes = hex::decode(blocktemplate_blob).unwrap();
        let mut block = deserialize::<monero::Block>(&bytes[..]).unwrap();
        let mut block_header = BlockHeader {
            version: 0,
            height: 0,
            prev_hash: FixedHash::zero(),
            timestamp: EpochTime::now(),
            output_mr: FixedHash::zero(),
            block_output_mr: FixedHash::zero(),
            output_smt_size: 0,
            kernel_mr: FixedHash::zero(),
            kernel_mmr_size: 0,
            input_mr: FixedHash::zero(),
            total_kernel_offset: Default::default(),
            total_script_offset: Default::default(),
            nonce: 0,
            pow: ProofOfWork::default(),
            validator_node_mr: FixedHash::zero(),
            validator_node_size: 0,
        };
        let hash = block_header.merge_mining_hash();
        insert_aux_chain_mr_and_info_into_block(&mut block, hash, 1, 0).unwrap();
        let count = 1 + (u16::try_from(block.tx_hashes.len()).unwrap());
        let mut hashes = Vec::with_capacity(count as usize);
        let mut proof = Vec::with_capacity(count as usize);
        hashes.push(block.miner_tx.hash());
        proof.push(block.miner_tx.hash());
        for item in block.clone().tx_hashes {
            hashes.push(item);
            proof.push(item);
        }
        let root = tree_hash(&hashes).unwrap();
        let coinbase_merkle_proof = create_merkle_proof(&hashes, &hashes[0]).unwrap();
        let aux_hashes = vec![monero::Hash::from_slice(hash.as_ref())];
        let aux_chain_merkle_proof = create_merkle_proof(&aux_hashes, &aux_hashes[0]).unwrap();

        // `monero::Transaction::default()` has an empty input vector, which no real coinbase has and which
        // `check_coinbase_prefix_bytes` refuses. Put the single `TxIn::Gen` back so this test fails on the thing
        // it is about - the missing merge mining tag - rather than on the fixture.
        let mut coinbase: monero::Transaction = Default::default();
        coinbase.prefix.inputs = vec![TxIn::Gen { height: VarInt(0) }];
        let extra = coinbase.prefix.extra.clone();
        let mut keccak = Keccak::v256();
        let mut encoder_prefix = Vec::new();
        coinbase.prefix.version.consensus_encode(&mut encoder_prefix).unwrap();
        coinbase
            .prefix
            .unlock_time
            .consensus_encode(&mut encoder_prefix)
            .unwrap();
        coinbase.prefix.inputs.consensus_encode(&mut encoder_prefix).unwrap();
        coinbase.prefix.outputs.consensus_encode(&mut encoder_prefix).unwrap();
        keccak.update(&encoder_prefix);

        let monero_data = MoneroPowData {
            header: block.header,
            randomx_key: FixedByteArray::from_canonical_bytes(&from_hex(&seed_hash).unwrap()).unwrap(),
            transaction_count: count,
            merkle_root: root,
            coinbase_merkle_proof,
            coinbase_prefix: CoinbasePrefix::Prefix(encoder_prefix.clone().try_into().unwrap()),
            coinbase_tx_extra: extra,
            aux_chain_merkle_proof,
        };
        let mut serialized = Vec::new();
        monero_data.serialize(&mut serialized).unwrap();
        let pow = ProofOfWork {
            pow_algo: PowAlgorithm::RandomXM,
            pow_data: PowData::try_from(serialized).unwrap(),
        };
        block_header.pow = pow;
        let err = verify_header(&block_header, &block_header.hash(), &rules).unwrap_err();
        unpack_enum!(MergeMineError::ValidationError(details) = err);
        assert!(details.contains("Expected merge mining tag was not found in Monero coinbase transaction"));
    }

    #[test]
    fn test_verify_header_no_data() {
        let rules = BaseNodeConsensusManager::builder(Network::LocalNet).build().unwrap();
        let mut block_header = BlockHeader {
            version: 0,
            height: 0,
            prev_hash: FixedHash::zero(),
            timestamp: EpochTime::now(),
            output_mr: FixedHash::zero(),
            block_output_mr: FixedHash::zero(),
            output_smt_size: 0,
            kernel_mr: FixedHash::zero(),
            kernel_mmr_size: 0,
            input_mr: FixedHash::zero(),
            total_kernel_offset: Default::default(),
            total_script_offset: Default::default(),
            nonce: 0,
            pow: ProofOfWork::default(),
            validator_node_mr: FixedHash::zero(),
            validator_node_size: 0,
        };
        // `monero::Transaction::default()` has an empty input vector, which no real coinbase has and which
        // `check_coinbase_prefix_bytes` refuses. Put the single `TxIn::Gen` back so this test fails on the thing
        // it is about - the missing merge mining tag - rather than on the fixture.
        let mut coinbase: monero::Transaction = Default::default();
        coinbase.prefix.inputs = vec![TxIn::Gen { height: VarInt(0) }];

        let extra = coinbase.prefix.extra.clone();
        let mut keccak = Keccak::v256();
        let mut encoder_prefix = Vec::new();
        coinbase.prefix.version.consensus_encode(&mut encoder_prefix).unwrap();
        coinbase
            .prefix
            .unlock_time
            .consensus_encode(&mut encoder_prefix)
            .unwrap();
        coinbase.prefix.inputs.consensus_encode(&mut encoder_prefix).unwrap();
        coinbase.prefix.outputs.consensus_encode(&mut encoder_prefix).unwrap();
        keccak.update(&encoder_prefix);

        let monero_data = MoneroPowData {
            header: Default::default(),
            // Any non-empty key: a zero length one is rejected by the deserializer, and this test is about the
            // missing merge mining tag.
            randomx_key: FixedByteArray::from_canonical_bytes(&[1u8; 32]).unwrap(),
            transaction_count: 1,
            merkle_root: Default::default(),
            coinbase_merkle_proof: create_merkle_proof(&[Hash::null()], &Hash::null()).unwrap(),
            coinbase_prefix: CoinbasePrefix::Prefix(encoder_prefix.clone().try_into().unwrap()),
            coinbase_tx_extra: extra,
            aux_chain_merkle_proof: Default::default(),
        };
        let mut serialized = Vec::new();
        monero_data.serialize(&mut serialized).unwrap();
        let pow = ProofOfWork {
            pow_algo: PowAlgorithm::RandomXM,
            pow_data: PowData::try_from(serialized).unwrap(),
        };
        block_header.pow = pow;
        let err = verify_header(&block_header, &block_header.hash(), &rules).unwrap_err();
        unpack_enum!(MergeMineError::ValidationError(details) = err);
        assert!(details.contains("Expected merge mining tag was not found in Monero coinbase transaction"));
    }

    #[test]
    fn test_verify_invalid_root() {
        let rules = BaseNodeConsensusManager::builder(Network::LocalNet).build().unwrap();
        let blocktemplate_blob = "0c0c8cd6a0fa057fe21d764e7abf004e975396a2160773b93712bf6118c3b4959ddd8ee0f76aad0000000002e1ea2701ffa5ea2701d5a299e2abb002028eb3066ced1b2cc82ea046f3716a48e9ae37144057d5fb48a97f941225a1957b2b0106225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa47820208f63aa86d2e857f070000".to_string();
        let seed_hash = "9f02e032f9b15d2aded991e0f68cc3c3427270b568b782e55fbd269ead0bad97".to_string();
        let bytes = hex::decode(blocktemplate_blob).unwrap();
        let mut block = deserialize::<monero::Block>(&bytes[..]).unwrap();
        let mut block_header = BlockHeader {
            version: 0,
            height: 0,
            prev_hash: FixedHash::zero(),
            timestamp: EpochTime::now(),
            output_mr: FixedHash::zero(),
            block_output_mr: FixedHash::zero(),
            output_smt_size: 0,
            kernel_mr: FixedHash::zero(),
            kernel_mmr_size: 0,
            input_mr: FixedHash::zero(),
            total_kernel_offset: Default::default(),
            total_script_offset: Default::default(),
            nonce: 0,
            pow: ProofOfWork::default(),
            validator_node_mr: FixedHash::zero(),
            validator_node_size: 0,
        };
        let hash = block_header.merge_mining_hash();
        insert_aux_chain_mr_and_info_into_block(&mut block, hash, 1, 0).unwrap();
        let count = 1 + (u16::try_from(block.tx_hashes.len()).unwrap());
        let mut hashes = Vec::with_capacity(count as usize);
        let mut proof = Vec::with_capacity(count as usize);
        hashes.push(block.miner_tx.hash());
        proof.push(block.miner_tx.hash());
        for item in block.clone().tx_hashes {
            hashes.push(item);
            proof.push(item);
        }

        let coinbase_merkle_proof = create_merkle_proof(&hashes, &hashes[0]).unwrap();
        let aux_hashes = vec![monero::Hash::from_slice(hash.as_ref())];
        let aux_chain_merkle_proof = create_merkle_proof(&aux_hashes, &aux_hashes[0]).unwrap();

        let coinbase = block.miner_tx.clone();
        let extra = coinbase.prefix.extra.clone();
        let mut keccak = Keccak::v256();
        let mut encoder_prefix = Vec::new();
        coinbase.prefix.version.consensus_encode(&mut encoder_prefix).unwrap();
        coinbase
            .prefix
            .unlock_time
            .consensus_encode(&mut encoder_prefix)
            .unwrap();
        coinbase.prefix.inputs.consensus_encode(&mut encoder_prefix).unwrap();
        coinbase.prefix.outputs.consensus_encode(&mut encoder_prefix).unwrap();
        keccak.update(&encoder_prefix);

        let monero_data = MoneroPowData {
            header: block.header,
            randomx_key: FixedByteArray::from_canonical_bytes(&from_hex(&seed_hash).unwrap()).unwrap(),
            transaction_count: count,
            merkle_root: Hash::null(),
            coinbase_merkle_proof,
            coinbase_prefix: CoinbasePrefix::Prefix(encoder_prefix.clone().try_into().unwrap()),
            coinbase_tx_extra: extra,
            aux_chain_merkle_proof,
        };
        let mut serialized = Vec::new();
        monero_data.serialize(&mut serialized).unwrap();
        let pow = ProofOfWork {
            pow_algo: PowAlgorithm::RandomXM,
            pow_data: PowData::try_from(serialized).unwrap(),
        };
        block_header.pow = pow;
        let err = verify_header(&block_header, &block_header.hash(), &rules).unwrap_err();
        unpack_enum!(MergeMineError::InvalidMerkleRoot = err);
    }

    #[test]
    fn test_difficulty() {
        // Taken from block: https://stagenet.xmrchain.net/search?value=672576
        let versions = "0c0c";
        // Tool for encoding VarInts:
        // https://gchq.github.io/CyberChef/#recipe=VarInt_Encode()To_Hex('Space',0)From_Hex('Auto'/disabled)VarInt_Decode(/disabled)&input=MTYwMTAzMTIwMg
        let timestamp = "a298b7fb05"; // 1601031202
        let prev_block = "046f4fe371f9acdc27c377f4adee84e93b11f89246a74dd77f1bf0856141da5c";
        let nonce = "FE394F12"; // 307182078
        let tx_hash = "77139305ea53cfe95cf7235d2fed6fca477395b019b98060acdbc0f8fb0b8b92"; // miner tx
        let count = "01";

        let input = from_hex(&format!("{versions}{timestamp}{prev_block}{nonce}{tx_hash}{count}")).unwrap();
        let key = from_hex("2aca6501719a5c7ab7d4acbc7cc5d277b57ad8c27c6830788c2d5a596308e5b1").unwrap();
        let rx = RandomXFactory::default();

        let (difficulty, hash) = get_random_x_difficulty(&input, &rx.create(&key, None, None).unwrap()).unwrap();
        assert_eq!(
            hash.to_hex(),
            "f68fbc8cc85bde856cd1323e9f8e6f024483038d728835de2f8c014ff6260000"
        );
        assert_eq!(difficulty.as_u64(), 430603);
    }

    #[test]
    #[serial]
    fn test_tari_randomx_difficulty() {
        let network = Network::Esmeralda;
        if std::env::var("TARI_NETWORK").is_err() {
            // SAFETY: This test is marked #[serial] and not run in parallel.
            unsafe { std::env::set_var("TARI_NETWORK", network.as_key_str()) };
        }
        if Network::get_current_or_user_setting_or_default() != network {
            let _ = Network::set_current(network);
        }
        let current_network = Network::get_current_or_user_setting_or_default();
        if current_network != network {
            panic!("could not set network");
        }
        let randomx_factory = RandomXFactory::new(1);
        let vm_key = from_hex("920647f8f10b8b484649adf83599c5b86bba137e7eb22e95dd8739d98528f677").unwrap();

        let vm = randomx_factory.create(vm_key.as_slice(), None, None).unwrap();

        let mut blob = 0u8.to_le_bytes().to_vec();

        blob.extend_from_slice(&[0u8; 1]);
        // timestamp
        blob.extend_from_slice(&[0u8; 1]);
        let mining_hash: Vec<u8> =
            Hex::from_hex("0ef6ed2c9c04830a899d388fbc5ec250acdb7b4b70fa2422c3d6e802c348d2c9").unwrap();
        blob.extend_from_slice(mining_hash.as_slice());
        blob.extend_from_slice(&[0u8; 4]);
        let mut prep_mining_blob = blob.clone();
        prep_mining_blob.extend_from_slice(&[0u8; 4]);
        // Add pow
        let pow = ProofOfWork {
            pow_algo: PowAlgorithm::RandomXT,
            pow_data: PowData::try_from(vec![0u8; 32]).unwrap(),
        };
        prep_mining_blob.extend_from_slice(&pow.to_bytes());

        assert_eq!(
            hex::encode(&prep_mining_blob),
            "0000000ef6ed2c9c04830a899d388fbc5ec250acdb7b4b70fa2422c3d6e802c348d2c90000000000000000020000000000000000000000000000000000000000000000000000000000000000"
        );

        let nonce = 8390400u64;
        // let nonce = 1u64;

        blob.extend_from_slice(hex::decode("00800700").unwrap().as_slice());
        blob.extend_from_slice(pow.to_bytes().as_slice());
        let difficulty = get_random_x_difficulty(&blob, &vm).unwrap();
        assert_eq!(
            hex::encode(&difficulty.1),
            "3b5af219f2491561cfd3b11d1ee9cc9fba81112f58042ce2f9c8541b2d44a410"
        );
        assert_eq!(difficulty.0.as_u64(), 15);

        // Now construct a block header and see if it matches
        let block_header = BlockHeader {
            version: 1,
            height: 123,
            prev_hash: FixedHash::try_from_slice(
                hex::decode("7d0b4f7dfcae9fbf72114f5b17d84691c7ba5061bb1cfb414964eceaf56d0157")
                    .unwrap()
                    .as_slice(),
            )
            .unwrap(),
            timestamp: 11222333.into(),
            output_mr: FixedHash::zero(),
            block_output_mr: FixedHash::zero(),
            output_smt_size: 0,
            kernel_mr: FixedHash::zero(),
            kernel_mmr_size: 0,
            input_mr: FixedHash::zero(),
            total_kernel_offset: Default::default(),
            total_script_offset: Default::default(),
            nonce,
            pow,
            validator_node_mr: FixedHash::zero(),
            validator_node_size: 0,
        };

        let mining_hash = block_header.mining_hash();
        assert_eq!(
            mining_hash.to_hex(),
            "0ef6ed2c9c04830a899d388fbc5ec250acdb7b4b70fa2422c3d6e802c348d2c9"
        );

        let diff = tari_randomx_difficulty(&block_header, &randomx_factory, &vm_key.try_into().unwrap()).unwrap();
        assert_eq!(diff.as_u64(), 15);
    }

    #[test]
    fn test_extra_field_deserialize() {
        let bytes = vec![
            3, 33, 0, 149, 5, 198, 66, 174, 39, 113, 243, 68, 202, 221, 222, 116, 10, 209, 194, 56, 247, 252, 23, 248,
            28, 44, 81, 91, 44, 214, 211, 242, 3, 12, 70, 0, 0, 0, 1, 251, 88, 0, 0, 96, 49, 163, 82, 175, 205, 74,
            138, 126, 250, 226, 106, 10, 255, 139, 49, 41, 168, 110, 203, 150, 252, 208, 234, 140, 2, 17, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        let raw_extra_field = RawExtraField(bytes);
        let res = ExtraField::try_parse(&raw_extra_field);
        assert!(res.is_err());
        let field = res.unwrap_err();
        let mm_tag = SubField::MergeMining(
            VarInt(0),
            Hash::from_slice(
                hex::decode("9505c642ae2771f344caddde740ad1c238f7fc17f81c2c515b2cd6d3f2030c46")
                    .unwrap()
                    .as_slice(),
            ),
        );
        assert_eq!(field.0[0], mm_tag);
    }

    // ---------------------------------------------------------------------------------------------------------
    // GHSA-3qmx-q9pv-f3m4 item 3: the strict `MerkleTreeParameters` decoding gate.
    // ---------------------------------------------------------------------------------------------------------

    /// The block template every fixture below borrows a well formed Monero header and coinbase from.
    const TEST_BLOCK_TEMPLATE_BLOB: &str = "0c0c8cd6a0fa057fe21d764e7abf004e975396a2160773b93712bf6118c3b4959ddd8ee0f76aad0000000002e1ea2701ffa5ea2701d5a299e2abb002028eb3066ced1b2cc82ea046f3716a48e9ae37144057d5fb48a97f941225a1957b2b0106225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa47820208f63aa86d2e857f070000";
    const TEST_SEED_HASH: &str = "9f02e032f9b15d2aded991e0f68cc3c3427270b568b782e55fbd269ead0bad97";

    /// A bit that no version of the decoder ever reads: the widest size field only takes the decoder up to bit 43.
    /// Setting it produces a varint that both decoders agree means the same thing, and that only the strict decoder
    /// can tell apart from the canonical form.
    const UNREAD_BIT: u64 = 1 << 50;

    /// The position `check_aux_chains` requires the Tari hash to sit at, recomputed here from the rule rather than
    /// borrowed from the function under test, so the fixture is independent of it.
    // The only arithmetic is `%`, and every caller passes a non-zero `number_of_chains`, so it cannot divide by zero.
    #[allow(clippy::arithmetic_side_effects)]
    fn expected_hash_position(genesis: &FixedHash, aux_nonce: u32, number_of_chains: u8) -> u32 {
        U256::from_little_endian(
            &Sha256::new()
                .chain_update(genesis)
                .chain_update(aux_nonce.to_le_bytes())
                .chain_update((109_u8).to_le_bytes())
                .finalize(),
        )
        .low_u32() %
            u32::from(number_of_chains)
    }

    /// Builds a two chain aux tree that `check_aux_chains` will accept on either side of *every* GHSA-3qmx-q9pv-f3m4
    /// fork: it searches for the nonce whose derived position matches where `tari_hash` actually sits in the tree,
    /// and the proof is a real two leaf proof, so its branch length and path bitmap also satisfy the aux-chain depth
    /// binding. Returns the nonce, the proof and the root.
    fn two_chain_aux_tree(tari_hash: &FixedHash, genesis: &FixedHash) -> (u32, MerkleProof, Hash) {
        let t_hash = Hash::from_slice(tari_hash.as_slice());
        let filler = Hash::from_slice(&[0x5au8; 32]);
        let aux_hashes = vec![t_hash, filler];
        let proof = create_merkle_proof(&aux_hashes, &t_hash).unwrap();
        // Asked for with the depth binding on, so a fixture that would trip that unrelated rule fails here rather
        // than inside the test it is used by.
        let (root, pos) = proof.calculate_root_with_pos(&t_hash, 2, true).unwrap();
        let aux_nonce = (0u32..10_000)
            .find(|n| expected_hash_position(genesis, *n, 2) == pos)
            .expect("a nonce landing on position {pos} exists well within 10 000 tries");
        (aux_nonce, proof, root)
    }

    /// A `MoneroPowData` whose only field `check_aux_chains` reads is the aux chain merkle proof. Everything else is
    /// taken from a real block template so the struct is well formed.
    fn pow_data_with_aux_proof(aux_chain_merkle_proof: MerkleProof) -> MoneroPowData {
        let bytes = hex::decode(TEST_BLOCK_TEMPLATE_BLOB).unwrap();
        let block = deserialize::<monero::Block>(&bytes[..]).unwrap();
        let coinbase_hash = block.miner_tx.hash();
        MoneroPowData {
            header: block.header,
            randomx_key: FixedByteArray::from_canonical_bytes(&from_hex(TEST_SEED_HASH).unwrap()).unwrap(),
            transaction_count: 1,
            merkle_root: Hash::null(),
            coinbase_merkle_proof: create_merkle_proof(&[coinbase_hash], &coinbase_hash).unwrap(),
            // Fed straight to `check_aux_chains`, which never reads the coinbase prefix.
            coinbase_prefix: CoinbasePrefix::Legacy(Keccak::v256()),
            coinbase_tx_extra: block.miner_tx.prefix.extra,
            aux_chain_merkle_proof,
        }
    }

    /// GHSA-3qmx-q9pv-f3m4 item 3. A differential run of `check_aux_chains` over the same non-canonical varint with
    /// the gate off and on. With the gate off it must be accepted exactly as it is today - that is the grandfathering
    /// guarantee - and with the gate on it must be rejected. The canonical varint must be accepted either way, so the
    /// gate is not simply rejecting everything.
    #[test]
    fn check_aux_chains_gate_accepts_a_non_canonical_varint_only_on_the_legacy_path() {
        let tari_hash = FixedHash::from([0x11u8; 32]);
        let genesis = FixedHash::from([0x22u8; 32]);
        let (aux_nonce, proof, root) = two_chain_aux_tree(&tari_hash, &genesis);
        let monero_data = pow_data_with_aux_proof(proof);

        let canonical = MerkleTreeParameters::new(2, aux_nonce).unwrap().to_varint();
        let non_canonical = VarInt(canonical.0 | UNREAD_BIT);
        assert_ne!(canonical, non_canonical);
        // Both decoders read the two varints identically; only canonicality separates them.
        assert_eq!(
            MerkleTreeParameters::from_varint_legacy(non_canonical.clone()),
            MerkleTreeParameters::from_varint_legacy(canonical.clone())
        );

        // The two GHSA-3qmx-q9pv-f3m4 gates that meet in this function share a flag day, so they are driven
        // together: `strict` also selects the aux-chain depth binding, which this fixture satisfies either way.
        let check = |params: VarInt, strict: bool| {
            check_aux_chains(
                &monero_data,
                params,
                &root,
                &tari_hash,
                &genesis,
                strict,
                strict,
                MAINNET_STRICT_MERKLE_TREE_PARAMS_ACTIVATION_HEIGHT,
            )
        };

        // The canonical encoding is accepted on both paths, so the fixture really is a valid aux chain setup.
        assert!(check(canonical.clone(), false).is_ok(), "canonical, legacy path");
        assert!(check(canonical.clone(), true).is_ok(), "canonical, strict path");

        // The non-canonical one is accepted below the fork and rejected at and above it.
        assert!(
            check(non_canonical.clone(), false).is_ok(),
            "non-canonical must still be accepted on the legacy path, or historical blocks are invalidated"
        );
        // The rejection is a `MerkleTreeParamsError` rather than the `ValidationError` every other rejection in
        // this function uses, because that variant is the one `get_ban_reason` maps to `None`. See the decode site.
        let err = check(non_canonical.clone(), true).unwrap_err();
        match &err {
            MergeMineError::MerkleTreeParamsError(MerkleTreeParametersError::NonCanonicalEncoding {
                varint,
                canonical: expected,
            }) => {
                assert_eq!(*varint, non_canonical.0);
                assert_eq!(*expected, canonical.0);
            },
            other => panic!("non-canonical must be rejected as a decode error, got {other:?}"),
        }
        assert!(
            err.get_ban_reason().is_none(),
            "a height disagreement about merge mining parameters must never ban the relaying peer"
        );
    }

    /// GHSA-3qmx-q9pv-f3m4 item 3. The out-of-range aux chain count is a verification failure rather than a panic or
    /// a clamp.
    ///
    /// Note this deliberately does not claim to be a differential test: on the legacy path that varint decodes to 255
    /// chains against a two leaf proof, so the position check rejects it there too. There is no gate difference to
    /// assert, only that the strict path surfaces it as a decode error rather than clamping it.
    #[test]
    fn check_aux_chains_treats_an_out_of_range_aux_chain_count_as_a_failure_on_the_strict_path() {
        let tari_hash = FixedHash::from([0x33u8; 32]);
        let genesis = FixedHash::from([0x44u8; 32]);
        let (_nonce, proof, root) = two_chain_aux_tree(&tari_hash, &genesis);
        let monero_data = pow_data_with_aux_proof(proof);

        // Size field 7 with every count bit set: 256 chains, which `number_of_chains` cannot hold.
        let out_of_range = VarInt((0b1111_1111u64 << 3) | 7);
        assert_eq!(
            MerkleTreeParameters::from_varint(out_of_range.clone()),
            Err(MerkleTreeParametersError::NumberOfChainsOutOfRange(256))
        );
        let err = check_aux_chains(
            &monero_data,
            out_of_range,
            &root,
            &tari_hash,
            &genesis,
            true,
            true,
            MAINNET_STRICT_MERKLE_TREE_PARAMS_ACTIVATION_HEIGHT,
        )
        .unwrap_err();
        assert!(
            matches!(
                &err,
                MergeMineError::MerkleTreeParamsError(MerkleTreeParametersError::NumberOfChainsOutOfRange(256))
            ),
            "expected an out-of-range aux chain count rejection, got {err:?}"
        );
    }

    /// A header at `height` whose Monero coinbase carries a merge mining tag with a NON-CANONICAL varint.
    ///
    /// The aux chain setup itself is valid - including under the aux-chain depth binding, which shares this fork's
    /// flag day - and the varint differs from the canonical one only in a bit no decoder reads, so both decoders
    /// resolve it to the same parameters. The coinbase merkle root is left deliberately invalid, so a header that
    /// gets *past* the aux chain check fails with `InvalidMerkleRoot` and one that does not fails earlier and
    /// differently. Returns the header and the genesis hash the aux tree was built against.
    fn non_canonical_header_at(rules: &BaseNodeConsensusManager, height: u64) -> (BlockHeader, FixedHash) {
        let mut block_header = BlockHeader {
            version: 0,
            height,
            prev_hash: FixedHash::zero(),
            timestamp: EpochTime::now(),
            output_mr: FixedHash::zero(),
            block_output_mr: FixedHash::zero(),
            output_smt_size: 0,
            kernel_mr: FixedHash::zero(),
            kernel_mmr_size: 0,
            input_mr: FixedHash::zero(),
            total_kernel_offset: Default::default(),
            total_script_offset: Default::default(),
            nonce: 0,
            pow: ProofOfWork::default(),
            validator_node_mr: FixedHash::zero(),
            validator_node_size: 0,
        };
        // The merge mining hash depends on the height, so the aux tree has to be rebuilt for each one.
        let tari_hash = block_header.merge_mining_hash();
        let genesis = FixedHash::from([0x66u8; 32]);
        let (aux_nonce, aux_chain_merkle_proof, root) = two_chain_aux_tree(&tari_hash, &genesis);

        let bytes = hex::decode(TEST_BLOCK_TEMPLATE_BLOB).unwrap();
        let mut block = deserialize::<monero::Block>(&bytes[..]).unwrap();
        insert_aux_chain_mr_and_info_into_block(&mut block, root, 2, aux_nonce).unwrap();
        // Rewrite the tag's varint into the non-canonical form. Both decoders read it as the same parameters,
        // so everything downstream of the decode is untouched.
        let mut extra_field = ExtraField::try_parse(&block.miner_tx.prefix.extra).unwrap();
        for item in &mut extra_field.0 {
            if let SubField::MergeMining(params, hash) = item {
                *item = SubField::MergeMining(VarInt(params.0 | UNREAD_BIT), *hash);
            }
        }
        block.miner_tx.prefix.extra = extra_field.into();

        let coinbase_hash = block.miner_tx.hash();
        let coinbase = block.miner_tx.clone();
        let mut encoder_prefix = Vec::new();
        coinbase.prefix.version.consensus_encode(&mut encoder_prefix).unwrap();
        coinbase
            .prefix
            .unlock_time
            .consensus_encode(&mut encoder_prefix)
            .unwrap();
        coinbase.prefix.inputs.consensus_encode(&mut encoder_prefix).unwrap();
        coinbase.prefix.outputs.consensus_encode(&mut encoder_prefix).unwrap();
        // GHSA-3qmx-q9pv-f3m4: which coinbase wire format is legal is a function of the height, so ask the same
        // question the node will ask rather than hard coding an answer. This fixture straddles the fork.
        let coinbase_prefix = match CoinbasePrefixMode::for_height(rules, height) {
            CoinbasePrefixMode::Legacy => {
                let mut keccak = Keccak::v256();
                keccak.update(&encoder_prefix);
                CoinbasePrefix::Legacy(keccak)
            },
            CoinbasePrefixMode::Derived => CoinbasePrefix::Prefix(encoder_prefix.try_into().unwrap()),
        };
        let monero_data = MoneroPowData {
            header: block.header,
            randomx_key: FixedByteArray::from_canonical_bytes(&from_hex(TEST_SEED_HASH).unwrap()).unwrap(),
            transaction_count: 1,
            // Deliberately wrong, so a header that gets past the aux chain check fails here instead.
            merkle_root: Hash::null(),
            coinbase_merkle_proof: create_merkle_proof(&[coinbase_hash], &coinbase_hash).unwrap(),
            coinbase_prefix,
            coinbase_tx_extra: block.miner_tx.prefix.extra,
            aux_chain_merkle_proof,
        };
        let mut serialized = Vec::new();
        monero_data.serialize(&mut serialized).unwrap();
        block_header.pow = ProofOfWork {
            pow_algo: PowAlgorithm::RandomXM,
            pow_data: PowData::try_from(serialized).unwrap(),
        };
        (block_header, genesis)
    }

    /// GHSA-3qmx-q9pv-f3m4 item 3. The gate is read from `header.height`, not from a constant: the *same* Monero
    /// data is accepted one block below MainNet's activation height and rejected at it.
    ///
    /// "Accepted" here means the aux chain check passed and `verify_header` went on to fail on the coinbase merkle
    /// root, which this fixture deliberately leaves invalid; "rejected" means it never got that far and reported the
    /// parameters as undecodable. The two errors are distinct, so the assertion cannot be satisfied by accident.
    #[test]
    fn verify_header_reads_the_strict_decoding_gate_from_the_header_height() {
        let rules = BaseNodeConsensusManager::builder(Network::MainNet).build().unwrap();
        let activation = MAINNET_STRICT_MERKLE_TREE_PARAMS_ACTIVATION_HEIGHT;
        assert!(
            !rules
                .consensus_constants(activation - 1)
                .strict_merkle_tree_parameter_decoding(),
            "the fixture assumes the gate is off below the activation height"
        );
        assert!(
            rules
                .consensus_constants(activation)
                .strict_merkle_tree_parameter_decoding(),
            "the fixture assumes the gate is on at the activation height"
        );

        // Below the fork: the aux chain check passes, so verification proceeds and trips on the coinbase root.
        let (header, genesis) = non_canonical_header_at(&rules, activation - 1);
        let err = verify_header(&header, &genesis, &rules).unwrap_err();
        unpack_enum!(MergeMineError::InvalidMerkleRoot = err);

        // At the fork: the strict decoder rejects the varint, and the error says so rather than claiming the tag was
        // missing, so whoever is on call is not sent looking for an absent tag.
        let (header, genesis) = non_canonical_header_at(&rules, activation);
        let err = verify_header(&header, &genesis, &rules).unwrap_err();
        match &err {
            MergeMineError::MerkleTreeParamsError(inner) => {
                assert!(
                    matches!(inner, MerkleTreeParametersError::NonCanonicalEncoding { .. }),
                    "expected a non-canonical encoding rejection, got {inner:?}"
                );
            },
            other => panic!("expected the aux chain check to fail at the activation height, got {other:?}"),
        }
        assert!(
            !err.to_string().contains("was not found"),
            "the tag was found and decoded; reporting it as missing misdirects whoever is on call: {err}"
        );

        // The regression guard that matters: this must NOT ban the peer. The relaying peer did not author the
        // varint and cannot alter it - it is committed to by the Monero proof of work - and it has no way to know
        // that we disagree with it about the activation height. Banning here would eclipse the upgraded minority by
        // banning the honest peers relaying the majority chain, and would hand any observer a free binary probe for
        // which build a node is running, because the pre-fork decoder can never produce this error at all.
        assert!(
            err.get_ban_reason().is_none(),
            "a height disagreement about merge mining parameters must never ban the relaying peer"
        );
    }

    /// GHSA-3qmx-q9pv-f3m4 item 3, shadow mode. Below the activation height the strict decoder runs for its
    /// diagnostic value only. This pins the property the whole feature depends on: the pre-fork outcome is
    /// completely unaffected by it.
    ///
    /// `check_aux_chains` is asserted directly, because that is the value shadow mode must not be able to touch, and
    /// `verify_header` is then run end to end below the fork so the shadow call actually executes on the real path.
    #[test]
    fn shadow_mode_does_not_change_the_legacy_outcome() {
        let tari_hash = FixedHash::from([0x77u8; 32]);
        let genesis = FixedHash::from([0x88u8; 32]);
        let (aux_nonce, proof, root) = two_chain_aux_tree(&tari_hash, &genesis);
        let monero_data = pow_data_with_aux_proof(proof);

        let canonical = MerkleTreeParameters::new(2, aux_nonce).unwrap().to_varint();
        let non_canonical = VarInt(canonical.0 | UNREAD_BIT);
        // Shadow mode is exactly this call, with the answer thrown away.
        assert!(
            MerkleTreeParameters::from_varint(non_canonical.clone()).is_err(),
            "the fixture must be a varint shadow mode would warn about"
        );
        shadow_check_merkle_tree_params(&non_canonical, 12_345, &FixedHash::from([0x99u8; 32]));

        // And the legacy path still accepts it, before and after the shadow call.
        assert!(
            check_aux_chains(
                &monero_data,
                non_canonical,
                &root,
                &tari_hash,
                &genesis,
                false,
                false,
                MAINNET_STRICT_MERKLE_TREE_PARAMS_ACTIVATION_HEIGHT - 1,
            )
            .is_ok(),
            "shadow mode must not change what the pre-fork decoder accepts"
        );

        // End to end below the fork. Note this fixture's coinbase merkle root is deliberately invalid, so it is
        // rejected before the shadow call is reached - which is now the intended behaviour: shadow mode reports
        // only on tags that actually passed merge mining verification, so that the report cannot be forged by a
        // peer sending a block that was never going to be accepted. What this pins is the property that matters
        // either way: the pre-fork outcome is exactly what it was before shadow mode existed.
        let rules = BaseNodeConsensusManager::builder(Network::MainNet).build().unwrap();
        let height = MAINNET_STRICT_MERKLE_TREE_PARAMS_ACTIVATION_HEIGHT - 1;
        assert!(
            !rules
                .consensus_constants(height)
                .strict_merkle_tree_parameter_decoding(),
            "the fixture assumes shadow mode, not strict mode"
        );
        let (header, genesis) = non_canonical_header_at(&rules, height);
        let err = verify_header(&header, &genesis, &rules).unwrap_err();
        unpack_enum!(MergeMineError::InvalidMerkleRoot = err);
    }

    // -----------------------------------------------------------------------------------------------------------
    // GHSA-3qmx-q9pv-f3m4: the rules height, not the header's claim, selects the verifiers
    // -----------------------------------------------------------------------------------------------------------

    /// A Tari header at `tari_height` carrying otherwise valid merge mining pow data, with
    /// `aux_chain_merkle_proof` set to `aux_proof`. Returns the header and the hash to pass as the genesis block
    /// hash (which is unused at depth 0, where there is only one aux chain and therefore no position to derive).
    ///
    /// The coinbase prefix is written in whichever wire format `tari_height` calls for, so this builds headers on
    /// both sides of the fork.
    fn merge_mined_header_with_aux_proof(
        rules: &BaseNodeConsensusManager,
        tari_height: u64,
        aux_proof: MerkleProof,
    ) -> (BlockHeader, FixedHash) {
        let blocktemplate_blob = "0c0c8cd6a0fa057fe21d764e7abf004e975396a2160773b93712bf6118c3b4959ddd8ee0f76aad0000000002e1ea2701ffa5ea2701d5a299e2abb002028eb3066ced1b2cc82ea046f3716a48e9ae37144057d5fb48a97f941225a1957b2b0106225b7ec0a6544d8da39abe68d8bd82619b4a7c5bdae89c3783b256a8fa47820208f63aa86d2e857f070000".to_string();
        let seed_hash = "9f02e032f9b15d2aded991e0f68cc3c3427270b568b782e55fbd269ead0bad97".to_string();
        let bytes = hex::decode(blocktemplate_blob).unwrap();
        let mut block = deserialize::<monero::Block>(&bytes[..]).unwrap();
        let mut block_header = BlockHeader::new(0);
        block_header.height = tari_height;

        // One aux chain, so `insert_aux_chain_mr_and_info_into_block` writes `SubField::MergeMining(VarInt(0), ..)`
        // - the depth 0 case this test is about. `merge_mining_hash` covers neither `pow` nor `pow_data`, so it
        // does not change when the pow data is attached below.
        let hash = block_header.merge_mining_hash();
        insert_aux_chain_mr_and_info_into_block(&mut block, hash, 1, 0).unwrap();

        let hashes = create_ordered_transaction_hashes_from_block(&block);
        let root = tree_hash(&hashes).unwrap();
        let coinbase_merkle_proof = create_merkle_proof(&hashes, &hashes[0]).unwrap();

        let coinbase = block.miner_tx.clone();
        let extra = coinbase.prefix.extra.clone();
        let mut encoder_prefix = Vec::new();
        coinbase.prefix.version.consensus_encode(&mut encoder_prefix).unwrap();
        coinbase
            .prefix
            .unlock_time
            .consensus_encode(&mut encoder_prefix)
            .unwrap();
        coinbase.prefix.inputs.consensus_encode(&mut encoder_prefix).unwrap();
        coinbase.prefix.outputs.consensus_encode(&mut encoder_prefix).unwrap();

        let coinbase_prefix = match CoinbasePrefixMode::for_height(rules, tari_height) {
            CoinbasePrefixMode::Legacy => {
                let mut keccak = Keccak::v256();
                keccak.update(&encoder_prefix);
                CoinbasePrefix::Legacy(keccak)
            },
            CoinbasePrefixMode::Derived => CoinbasePrefix::Prefix(encoder_prefix.try_into().unwrap()),
        };

        let monero_data = MoneroPowData {
            header: block.header,
            randomx_key: FixedByteArray::from_canonical_bytes(&from_hex(&seed_hash).unwrap()).unwrap(),
            transaction_count: u16::try_from(hashes.len()).unwrap(),
            merkle_root: root,
            coinbase_merkle_proof,
            coinbase_prefix,
            coinbase_tx_extra: extra,
            aux_chain_merkle_proof: aux_proof,
        };
        let mut serialized = Vec::new();
        monero_data.serialize(&mut serialized).unwrap();
        block_header.pow = ProofOfWork {
            pow_algo: PowAlgorithm::RandomXM,
            pow_data: PowData::try_from(serialized).unwrap(),
        };
        (block_header, hash)
    }

    /// The empty proof an honest single-aux-chain miner produces. `construct_monero_data` builds it with
    /// `create_merkle_proof` over a one-leaf tree, which is exactly this - and it is what `check_aux_chains`
    /// requires of a `VarInt(0)` tag once the aux-chain depth binding is active.
    fn honest_single_chain_aux_proof() -> MerkleProof {
        let leaf = vec![monero::Hash::from_slice(&[0x11; 32])];
        let proof = create_merkle_proof(&leaf, &leaf[0]).unwrap();
        assert!(
            proof.branch().is_empty() && proof.path() == 0,
            "an honest depth 0 proof carries nothing"
        );
        proof
    }

    /// The gossip gate half of the advisory follow up: the pre-validation gate looks at an unlinked header, so
    /// `header.height` is a peer's assertion. Selecting the rules from it let a peer name a pre-fork height and be
    /// handed the pre-fork verifiers - including the forgeable Keccak sponge - at zero cost.
    ///
    /// `verify_header_at_rules_height` is what lets the gate pick the rules from a height it can corroborate
    /// instead. This pins that the height argument, not the header's own field, is what decides.
    #[test]
    fn the_rules_height_overrides_the_headers_claim() {
        let rules = BaseNodeConsensusManager::builder(Network::Esmeralda).build().unwrap();
        let activation = ESMERALDA_DERIVED_MONERO_COINBASE_ACTIVATION_HEIGHT;

        // A header written in the pre-fork wire format, claiming a pre-fork height. Believing the claim - which
        // is what `verify_header` does, and what the gate used to do - validates it.
        let (header, gen_hash) = merge_mined_header_with_aux_proof(&rules, 0, honest_single_chain_aux_proof());
        verify_header(&header, &gen_hash, &rules).unwrap();

        // The same bytes, judged at a height the node can corroborate - its own tip, at or above the fork. The
        // legacy coinbase format is no longer legal there, so the sponge never gets looked at.
        let err = verify_header_at_rules_height(&header, &gen_hash, &rules, activation).unwrap_err();
        assert!(matches!(err, MergeMineError::DeserializeError(_)), "{err}");

        // And a post-fork header judged at its own height still validates, so the override is not simply
        // rejecting everything: `max(tip, claimed)` on an honest announcement is the claimed height.
        let (post_fork, gen_hash) =
            merge_mined_header_with_aux_proof(&rules, activation, honest_single_chain_aux_proof());
        verify_header_at_rules_height(&post_fork, &gen_hash, &rules, activation).unwrap();

        // A peer that lies the *other* way, claiming a height above our tip, gets rules at least as strict as
        // ours, never weaker - which is why taking the max is safe.
        let claimed = activation + 10;
        let (post_fork, gen_hash) = merge_mined_header_with_aux_proof(&rules, claimed, honest_single_chain_aux_proof());
        verify_header_at_rules_height(&post_fork, &gen_hash, &rules, max(activation, claimed)).unwrap();
    }
}
