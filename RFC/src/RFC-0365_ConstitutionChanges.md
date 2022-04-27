# RFC-0365/ConstitutionChanges

## Constitution Changes

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
The aim of this document is to describe the mechanisms of contract constitution changes on the Tari Digital Asset network (DAN).

## Related Requests for Comment
* [RFC-0312: High level Digital Asset Network Specification](RFC-0312_DANHighLevelSpecification.md)

## Description

### Overview
After the [contract definition transaction], the asset issuer publishes the [contract constitution transaction]. That transaction (UTXO) defines how a contract is managed, including, among others:
1. Validator Node Committee (VNC) composition. This includes the rules over how members are added or removed. It may be that the VNC has autonomy over these changes, or that the asset issuer must approve changes, or some other authorization mechanism.
2. Side-chain medatada record: the consensus algorithm to be used and the checkpoint quorum requirements.
3. Checkpoint parameters record: minimum checkpoint frequency, comitee change rules (e.g. asset issuer must sign, or a quorum of VNC members, or a whitelist of keys).

Then, and ONLY during the contract execution, any authorsised VNC can propose a change of any of those three parameters. The constitution changes MUST happen at checkpoints.

Note that changes (how and when) in both side-chain metadata record and checkpoint parameters record MAY be specified in the contract constitution UTXO, inside a `RequirementsForConstitutionChange` record. If omitted, the checkpoint parameters and side-chain metadata records are immutable via covenant.

### Stages of a constitution change
A constitution change is composed of multiple stages. Each stage produces ONE checkpoint transaction into the base layer. The base layer MUST confirm at checkpoints that the requirements specified in the contract constitution are met.

While a constitution change proposal is not finished (i.e. the [activation stage] hasn't finished yet), the contract is still in execution so the VNC MAY produce any number of regular checkpoints transactions during that time.

ANY number of constitution changes MAY happend in a sequence fashion. But ONLY ONE constitution change process can be active at a time, to prevent conflicting changes to be accepted simultaneously.

#### Proposal
[proposal]: #proposal

During contract execution, any authorised change proposer, as defined in the contract constitution, can propose constitution changes. More specifically, the [contract constitution] defines in the [checkpoint parameters record] the rules for a valid change proposer. It can be ONLY ONE of the following:
* The asset issuer is the ONLY authorised for consitution changes, so it MUST sign the constitution change transaction.
* A quorum of VNC members is needed for constitution changes, signed via a multisig with at least the minimum required amount of members.
* A list of public keys that each one have signing power over a constitution change transaction.

The proposer MUST sign and publish a constitution change proposal transaction. The change proposal UTXO:
* MUST include the contract id.
* MUST include a constitution change proposal id, to differentiate between multiple change proposals.
* MUST include the `ConstitutionChangeProposal` output flag.
* MAY include the [side-chain metadata record], to specify a new consensus algorithm and/or a new checkpoint quorum requirement. 
* MAY include a [checkpoint parameters record], to specify a new minimum checkpoint frequency or new commitee change rules.
* MUST include an expiry timestamp before which all VNs must sign and agree to the new terms.

#### Acceptance
After the [proposal], each VNC member validates the [consitution change proposal transaction]:
* Validates that the proposer has the right to propose a constitution change. It is defined in the [contract constitution] inside the [checkpoint parameters record].
* Validates that the proposed changes on side-chain metadata record and checkpoint parameters record, if present, MUST align with the `RequirementsForConstitutionChange` record present in the [contract constitution]. If the `RequirementsForConstitutionChange` does not exist, no changes are allowed (they are considered immutable by default).
* Validates that there is no other VALID proposal still in progress (i.e. the expiry timestamp hasn't passed yet).

At this point, if all the checks are successful the proposal is consiered `VALID`, but not yet `ACCEPTED`.

Each VNC member, if they considerd the proposal as `VALID` and they accept the new constitution changes, MUST publish a constitution change acceptance transaction. The transaction format is similar to the [initial contract acceptance transaction], including the output feature `ContractAcceptance`, the only difference is that it includes the constitution change proposal id. The UTXO of the potential VNC member MUST stake the required funds via a time-lock, in this case until the end of the [constitution change activation] period.

At the end of the expiry timestamp (specified in the [constitution change proposal]), if not enough quorum validates the proposal, the constitution change cycle ends.

#### Activation
At this point, there MUST be a quorum of acceptance transactions from validator nodes. The validator node committee MUST collaborate to produce, sign and broadcast the constitution change activation transaction:
* The transaction MUST spend all the [change acceptance transactions] UTXOs for the contract.
* Base layer consensus MUST confirm that the spending rules and covenants have been observed, and that the checkpoint contains the correct covenants and output flags.
* This transaction also acts as a regular checkpoint for the contract. As such, it requires all the checkpoint information.

### Use case example
main use case is the committee changing

### Contract constitutions for proof-of-work side-chains
Miners are joining and leaving PoW chains all the time. It is impractical to require a full constitution change cycle to execute every time this happens, the chain would never make progress!

To work around this, the constitution actually defines a set of proxy- or observer-nodes that perform the role of running a full node on the side chain and publishing the required [checkpoint transaction]s onto the Tari base chain. The observer node(s) are then technically the VNC. Issuers could place additional safeguards in the contract definition and constitution to keep the VNC honest. Conceivably, even Monero or Bitcoin itself could be attached as a side-chain to Tari in this manner.