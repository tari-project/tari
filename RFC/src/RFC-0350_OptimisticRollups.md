# RFC-0350/OptimisticRollups

## Optimistic Rollups

![status: draft](theme/images/status-draft.svg)

**Maintainer(s)**: [Miguel Naveira](https://github.com/mrnaveira)

# Licence

[ The 3-Clause BSD Licence](https://opensource.org/licenses/BSD-3-Clause).

Copyright 2022. The Tari Development Community

Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
following conditions are met:

1. Redistributions of this document must retain the above copyright notice, this list of conditions and the following
   disclaimer.
2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following
   disclaimer in the documentation and/or other materials provided with the distribution.
3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote products
   derived from this software without specific prior written permission.

THIS DOCUMENT IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS", AND ANY EXPRESS OR IMPLIED WARRANTIES,
INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
WHETHER IN CONTRACT, STRICT LIABILITY OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF
THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

## Language

The keywords "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", 
"NOT RECOMMENDED", "MAY" and "OPTIONAL" in this document are to be interpreted as described in 
[BCP 14](https://tools.ietf.org/html/bcp14) (covering RFC2119 and RFC8174) when, and only when, they appear in all capitals, as 
shown here.

## Disclaimer

This document and its content are intended for information purposes only and may be subject to change or update
without notice.

This document may include preliminary concepts that may or may not be in the process of being developed by the Tari
community. The release of this document is intended solely for review and discussion by the community regarding the
technological merits of the potential system outlined herein.

## Goals

The goal of this Request for Comment (RFC) is to propose an Optimistic Rollup implementation in the Tari Digital Asset Network (DAN) layer.

## Related Requests for Comment

* [RFC-0340: Validator Node Consensus](RFC-0340_VNConsensusOverview.md)

## Description

### Overview

Rollups are a scalability mechanism for layer 2. They allow offloading heavy computation from a main blockchain into a side-chain. Rollups can bundle a large amount of transactions into a single transaction into layer 1, significantly boosting performance.

There are two approaches to rollups:
* **Optimistic Rollups**:
    * All transactions are assumed valid by default, but they can be challenged by any party (via submitting a _fraud proof_) if considered fraudulent or erroneous.
    * Stakes on both the rollup submitter and the challenger aligns incentives so that a single honest party can force the side-chain to behave correctly.
    * In the case of a dispute, the work done in the rollup must be computed (totally in a non-interactive approach, or partially in an interactive one) in the layer 1 to decide if the transaction is correct or not and execute the stake slashing and rewarding for the parties involved.
    * Requires long wait times for transaction confirmation to allow for a challenge window, usually around 1 week. There are currently efforts being made to allow fast withdrawals via liquidity exits.
* **Zero-knoledge Rollups**:
    * Submits _validity proofs_ with the rollup into layer 1, so that the computation can be immediately considered valid without redoing all the work in the rollup.
    * This approach makes it unnecessary to implement a dispute mechanism or a challenge window.
    * The main con is that validity proofs are hard to create and very application specific at the moment.

This document describes an implementation of **Optimistic Rollups** in the Tari network. For clarity:
* _Checkpoint_: periodical transactions in the base layer done by the VNC to summarize the state of a side-chain. It's an equivalent to a rollup for this document.
* _Submitter_: the party that submits a checkpoint, i.e. a quorum-valid subset of the VNC.
* _Challenger_: the party that claims that a checkpoint is erroneous or fraudulent. It will usually be a VNC member (it could be the asset issuer for example, if it participates in the committee).
* _Fraud proof_: the data provided by the _challenger_ to the base layer in order to demonstrate that a particular checkpoint is invalid.
* _Challenge window_: amount of time since the publication of a checkpoint in which a fraud proof can be accepted. Let's assume a 1-week window.
* _Stake_: monetary amount that both the _submitter_ and the _challenger_ offer to back up their (opposing) claims. Let's assume the winner takes it all.
* Mapping concepts from common rollup literature in Tari: 
    * The smart contract code being run in the side-chain are _templates_.
    * Transactions in base layer are referred simply as transactions, but transactions inside the side-chain will be referred as _instructions_.
    * The VM state is called a _view_.

Trough this RFC, the approach taken was to prioritize safety first and then easiness of implementation second. This means moving up responsibilities (and thus, bloating) the base layer. There are less costly options that are inherently less costly in the base layer, but at the expense of safety and/or simplicity. These alternatives are briefly mentioned in the document without going into detail. 

### Checkpoint structure

DAN layer side-chains in the Tari network periodically submit rollups into the base layer via checkpoint transactions. Checkpoint frequency is determined in the contract constitution, so each side-chain can choose a custom value.

A checkpoint transaction will live as an UTXO in the base layer. It MUST have the `sidechain_checkpoint` output flag and include the following data fields:

| Field                              | Type                      | Description |
| ---------------------------------- | ------------------------- | ----------- |
| `contract_id`                      | Keccak 256 hash           | Immutable contract ID, calculated as a hash of the contract's definition fields|
| `checkpoint_unique_id`             | `u64`                     | Unique identifier for this checkpoint inside the contract's scope |
| `previous_checkpoint_unique_id`    | `u64`                     | Reference to the previous checkpoint in the sequence. Needed for disambiguation in case of conflicting checkpoints |
| `previous_checkpoint_block_height` | `u64`                     | Needed for fast access by the base layer nodes in fraud proof validation |
| `instruction_merkle_root`          | Keccak 256 hash           | Hash of all the instructions being processed since the last checkpoint |
| `view_merkle_root`                 | Keccak 256 hash           | Hash of the state of the contract template represented by the checkpoint |
| `committee`                        | List of Public keys       | List of all the Ristretto's public keys for the committee members signing this checkpoint |

The checkpoint UTXO MUST stake at least the minimum amount of Tari specified in the contract constitution. This minimum amount must be carefully selected so the cost of submitting all the info for a fraud proof is less than the stake rewarded by it, that way honest VNs are economically incentivized to challenge fraudulent checkpoints.

The stake will be spent in ONE of the following cases:
* A challenger publishes a fraud proof that demonstrates that the checkpoint is fraudulent, so the submitter's staked amount is sent to the challenger.
* The challenge window (let's assume 1 week) expires without any valid fraud proof, in that case the stake can be spent back by checkpoint submitter.

Also, the checkpoint transaction MUST contain a valid multi-signature of all the specified committee members. The base layer MUST check at each checkpoint that the signature is valid, matches all the committee members specified, and the quorum requirements are met.

### Issuing a fraud proof

Any honest party in the network can be monitoring the checkpoints being published in the base layer and submit a fraud proof to challenge them. The fraud proof submitter will usually be a VNC member that detects an erroneous or fraudulent behavior in the rest of the VNC.

A fraud proof consists of an UTXO in the base layer. It MUST have the `fraud_proof` output flag and include the following data fields:

| Field                              | Type                    | Description |
| ---------------------------------- | ----------------------- | ----------- |
| `contract_id`                      | Keccak 256 hash         | Immutable contract ID, calculated as a hash of the contract's definition fields|
| `checkpoint_block_height`          | `u64`                   | Needed for fast access by the base layer nodes in fraud proof validation |
| `checkpoint_unique_id`             | `u64`                   | Unique identifier of the checkpoint being challenged |
| `initial_view_state`               | ?                       | The full state of the template execution in the previous checkpoint
| `instructions`                     | `Vec<Instruction>`      | A vector containing all the ordered instructions that happened since the previous checkpoint of the one being challenged |
| `challenger_public_key`            | Public key              | Ristretto's public key of the challenger VN |

The fraud proof UTXO MUST stake at least the minimum amount of Tari specified in the contract constitution, the same amount as the checkpoint. It will only be unlocked to be spent if the base layer validates the fraud proof as correct.

The size of the `initial_view_state` and the `instructions` field could be potentially huge. After the fraud proof is considered either correct or incorrect by the base layer, the UTXO will be spent so all that data could be pruned from the blockchain.

We left out the potentially correct view state or merkle root from the fraud proof. As the base layer MUST recalculate it anyway, and we are only interested in if it matches with the merkle root of the challenged checkpoint, there is no point in including it in the fraud proof. Applying all the `instructions` to the `initial_view_state` should get us the correct view state of the challenged checkpoint.

Also, the checkpoint transaction MUST contain a valid signature of the challenger.

#### Open questions:
* With the proposed approach, all the view state and the instructions need to be stored in the fraud proof, on-chain. To avoid making it too costly, an off-chain decentralized storage solution could be implemented to make fraud proofs only include references to the raw data, but the base layer must implement a protocol to retrieve and check that off-chain data. 
* The proposal assumes that the fraud proofs are non-interactive, that means the whole work done since the last checkpoint must be redone in the base layer to determine if a checkpoint is fraudulent or not. Non-interactive fraud proofs are the simplest implementation. There are also more sophisticated protocols that make fraud proof interactive, meaning that both the challenger and the challenged parties collaborate to create a fraud proof with only the individual disputed instructions to be checked by the base layer.

### Validating fraud proofs

The validation of fraud proofs MUST be done by base layer, to leverage the security and decentralization that it provides over the side-chains. The goal is to determine if the checkpoint being challenged is fraudulent or not, and transfer the stakes to the winning party.

To determine if a fraud proof is valid, the base layer MUST check, in order, that:
1. The signature in the proof UTXO matches the specified public key.
2. The challenge window for the checkpoint has not expired yet.
3. The `initial_view_state` in the proof corresponds to the previous checkpoint to the one being challenged. It needs to retrieve all the raw view data provided in the fraud proof, calculate the hash and compare it to the one included in the previous checkpoint.
4. The `instructions` in the proof corresponds to the ones in the challenged checkpoint. It needs to retrieve all the raw instruction data provided in the fraud proof, calculate the merkle tree root and compare it with the one in the challenged checkpoint.
5. The final view does not match the one in the challenged checkpoint. To check that, the base layer must apply ALL the instructions to the initial view to calculate the final view, calculate the hash of that final view and compare it to the one being provided in the checkpoint.

If all the previous checks are valid the fraud proof is considered valid and the challenged checkpoint is considered fraudulent. Otherwise, if any of the checks are invalid, the fraud proof is considered invalid and the checkpoint is still valid.

Note that it's not needed to check if a checkpoint was already considered fraudulent in the past. This is because the UTXO of a fraudulent checkpoint will be spent by the successful challenger.

#### Open questions:
* How are templates and instructions going to be implemented? This is a heavy dependency for optimistic rollups, as all the computations MUST be deterministic to be reproduced in a fraud proof.
* Currently, the base layer does not have the tools to reproduce the computations being done in the side-chains. This requires the base layer to be able to execute template code when a `fraud_proof` output flag is present in a transaction. This is a huge extension to the base layer. An alternative could be to move the validation off-chain (as some implementations do), in this case to a wider set of VNs outside the contract VNC, with the proper economical incentives.
* How do we handle the dispute over the instructions themselves? Instructions could be signed by the user emitting them, so fake instructions can be checked, but there is the case of a challenger claiming that a checkpoint censored or reordered one or more instructions. This last case is not possible to verify in the base layer with the current proposal. Many implementations of optimistic rollups rely on the instructions being stored on-chain to solve this.

### Checkpoint sequence forks

There is a particular scenario that can happen if two (or more) competing subsets of the VNC disagree, and the particular quorum constraints of the contract allow them to publish competing checkpoints. From that point onwards, the sequence of checkpoints will fork into two (or more) separate paths forming a tree instead of a sequence.

In this case, the honest subset of the VNC is heavily incentivized to challenge the other's checkpoints to obtain their stakes. The more an invalid branch of checkpoints is continued, the greater the rewards for challenging them are. The base layer will ultimately discard all the invalid checkpoints via validating fraud proofs.

#### Open questions:
* Could it be better if the base layer checks at every checkpoint that a sequence number increases by 1? This way a fork will never happen, and the honest party is forced to publish a fraud proof as soon as possible. The downside is that this means bloating the base layer with more checks at each checkpoint, as well as stopping further honest checkpoints to be submitted until the fraudulent one is invalidated by a fraud proof.

### Fraud proof resolution

After fraud proof validation, the base layer will determine if the checkpoint was fraudulent or not.

If the fraud proof is valid, the checkpoint is considered fraudulent. In that case:
* The stake in the checkpoint UTXO (that contains the staked amount) CAN be spent ONLY by the challenger in a new transaction.
* The stake in the fraud proof is unlocked and CAN be spent ONLY by the challenger.
* All subsequent checkpoints that follow the invalidated one will be invalid as well. For the sake of reusability, they MUST be individually challenged.

If the fraud proof is invalid, the checkpoint is not considered fraudulent, so:
* The stake in the checkpoint UTXO (that contains the staked amount) is unlocked and can be spent by the multi-signature of the submitters.
* The stake in the fraud proof CAN be spent ONLY by the multi-signature of the submitters.

#### Open questions:
* Exactly how are the stakes unlocked and allowed to be spent by the winning party? Do covenants and/or scripts allow it?
* In some implementations, the stake of the losing side is slashed in half and the other half is sent to the winner. Needs further investigation on why it's like that and not a "winner takes all" approach.
* Is it convenient to implement a liquidity exit to allow to consider a checkpoint data as confirmed before the challenge window expires?