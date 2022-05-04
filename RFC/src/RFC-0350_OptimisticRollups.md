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

## Related Requests for Comment

* [RFC-0311 Asset templates](RFC-0311_AssetTemplates.md)
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

### Checkpoint structure

DAN layer side-chains in the Tari network periodically submit rollups into the base layer via checkpoint transactions. Checkpoint frequency is determined in the contract constitution, so each side-chain can choose a custom value.

In order for the base layer to resolve any dispute, the checkpoint must include:
* A merkle tree root of all the instructions applied since the last checkpoint.
* A hash of the final state (`view`) of the contract execution.
* Signatures of all the VNC members submitting the checkpoint.
* Stake to be slashed if the checkpoint is considered fraudulent in the future.

The checkpoint will live as a UTXO in the base layer, and will be spent in one of the following cases:
* A challenger publishes a fraud proof that demonstrates that the checkpoint is fraudulent, so the submitter's staked amount is sent to the challenger.
* The challenge window (let's assume 1 week) expires without any valid fraud proof, in that case the stake can be spent back by checkpoint submitter.

### Issuing a fraud proof

Any honest party in the network can be monitoring the checkpoints being published in the base layer and submit a fraud proof to challenge them. The fraud proof submitter will usually be a VNC member that detects an erroneous or fraudulent behavior in the rest of the VNC.

The fraud proof consists of a transaction in the base layer. It MUST contain all the information for the base layer to determine if a checkpoint is fraudulent or not:
* A reference to the checkpoint being challenged.
* The initial state (`view`), corresponding to the state represented in the previous checkpoint of the one being challenged.
* All the instructions that were executed since the previous checkpoint of the one being challenged.
* Stake of the challenger, to be slashed if the fraud proof is invalid.

The size of the initial state and the instruction collection could be potentially huge. To avoid making it too costly, an off-chain solution could be implemented to make fraud proofs only include references (URLs?) to the raw data for those fields, but the base layer must implement a protocol to retrieve and check that off-chain data.

In this document we assume that the fraud proofs are non-interactive, that means the whole initial state and all the instructions must be checked by the base layer to determine if a checkpoint is fraudulent or not. Non-interactive fraud proofs are the simplest implementation. There are also more sophisticated protocols that make fraud proof interactive, meaning that both the challenger and the challenged parties collaborate to create a fraud proof with only the individual disputed instructions to be checked by the base layer.

### Validating fraud proofs

The validation of fraud proofs MUST be done by base layer, to leverage the security and decentralization that it provides over the side-chains. The goal is to determine if the checkpoint being challenged is fraudulent or not, and transfer the stakes to the winning party.

To determine if a fraud proof is valid, the base layer MUST:
* Check that the challenge window for the checkpoint has not expired yet.
* Check that the checkpoint was not already confirmed fraudulent via a previous fraud proof.
* Check that the initial state in the proof corresponds to the previous checkpoint to the one being challenged. It needs to retrieve all the raw view data provided in the fraud proof, calculate the hash and compare it to the one included in the previous checkpoint.
* Check that the instructions in the proof corresponds to the ones in the challenged checkpoint. It needs to retrieve all the raw instruction data provided in the fraud proof, calculate the merkle tree root and compare it with the one in the challenged checkpoint.
* Apply ALL the instructions to the initial view to calculate the final view, calculate the hash of that final view and compare it to the one being provided in the checkpoint, it must be different to the challenged checkpoint.

If all the previous checks are valid the fraud proof is considered valid, so the checkpoint is considered fraudulent:
* The stake in the checkpoint is transferred to the challenger party.
* The stake in the fraud proof is unlocked and can be spent.
* All subsequent checkpoints that follow the invalidated one will be invalid as well. For the sake of reusability, they MUST be individually challenged.

Otherwise, if any of the checks are invalid, the fraud proof is considered invalid, so the stakes in the fraud proof are transferred to the checkpoint submitters.

### Checkpoint sequence forks

There is a particular scenario that can happen if two (or more) competing subsets of the VNC disagree, and the particular quorum constraints of the contract allow them to publish competing checkpoints. From that point onwards, the sequence of checkpoints will fork into two (or more) separate paths forming a tree instead of a sequence.

In this case, the honest subset of the VNC is heavily incentivized to challenge the other's checkpoints to obtain their stakes. The more an invalid branch of checkpoints is continued, the greater the rewards for challenging them are. The base layer will ultimately discard all the invalid checkpoints via validating fraud proofs.

### Open questions:
* Currently, the base layer does not have the tools to reproduce the computations being done in the side-chains. We need a way to execute templates in the base layer for validating fraud proofs.
* How do we handle the dispute over the instructions themselves? Instructions could be signed by the user emitting them, so fake instructions can be checked, but there is the case of a challenger claiming that a checkpoint censored or eliminated one or more instructions. This last case is not possible to verify in the base layer with the current proposal.
* Are all the raw data for the initial state and the instructions in a fraud proof included on-chain or off-chain? If off-chain how, via URLs?
* Do we implement a non-interactive fraud proof as proposed, or do we go for a more complicated (but more efficient) interactive way?
* Is it convenient to implement a liquidity exit to allow to consider a checkpoint data as confirmed before the challenge window expires?
* Regarding the checkpoint sequence fork scenario, could it be better if the base layer checks at every checkpoint that a sequence number increases by 1? This way a fork will never happen, and the honest party is forced to publish a fraud proof as soon as possible. The downside is that this means bloating the base layer with more checks at each checkpoint.