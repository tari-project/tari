# RFC-0340/VNConsensusOverview

## Validator node consensus algorithm

![status: draft](theme/images/status-draft.svg)

**Maintainer(s)**: Cayle Sharrock <CjS77>

# License

[ The 3-Clause BSD License](https://opensource.org/licenses/BSD-3-Clause).

Copyright 2019. The Tari Development Community
## Validator node consensus algorithm

![status: draft](theme/images/status-draft.svg)

**Maintainer(s)**: Cayle Sharrock <CjS77>

# License

[ The 3-Clause BSD License](https://opensource.org/licenses/BSD-3-Clause).

Copyright 2019. The Tari Development Community

Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
following conditions are met:

1. Redistributions of this document must retain the above copyright notice, this list of conditions and the following
   disclaimer.
2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following
   disclaimer in the documentation and/or other materials provided with the distribution.
3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote products
   derived from this software without specific prior written permission.

THIS DOCUMENT IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF
THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
following conditions are met:

1. Redistributions of this document must retain the above copyright notice, this list of conditions and the following
   disclaimer.
2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following
   disclaimer in the documentation and/or other materials provided with the distribution.
3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote products
   derived from this software without specific prior written permission.

THIS DOCUMENT IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF
THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

## Language

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED",
"NOT RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in
[BCP 14](https://tools.ietf.org/html/bcp14) (covering RFC2119 and RFC8174) when, and only when, they appear in all capitals, as
shown here.

## Disclaimer

The purpose of this document and its content is for information purposes only and may be subject to change or update
without notice.

This document may include preliminary concepts that may or may not be in the process of being developed by the Tari
community. The release of this document is intended solely for review and discussion by the community regarding the
technological merits of the potential system outlined herein.

## Goals

This document describes at a high level how smart contract state is managed on the Tari Digital Assets Network.

## Related RFCs

* [RFC-0300: The Tari Digital Assets Network](RFCD-0300_DAN.md)
* [RFC-0302: Validator Nodes](RFCD-0302_ValidatorNodes.md)
* [RFC-0304: Validator Node committee selection](RFC-0304_VNCommittees.md)
* [RFC-0341: Asset Registration](RFC-0341D_AssetRegistration.md)

## Description

### Overview

The primary problem under consideration here is for multiple machines running the same program (in the form of a Tari
smart contract) to maintain agreement on what the state of the program is, often under adverse conditions, including
unreliable network communication, malicious third parties, or even malicious peers running the smart contract.

In computer science terms, the problem is referred to as
[State Machine Replication](https://en.wikipedia.org/wiki/State_machine_replication), or SMR. If we want our honest
machines (referred to as _replicas_ in SMR parlance) to reach agreement in the face of arbitrary failures, then we talk
about our system being
[Byzantine Fault Tolerant](https://tlu.tarilabs.com/consensus-mechanisms/BFT-consensusmechanisms/sources/PITCHME.link.html).

Tari Asset [committees] are chosen by the asset issuer according to [RFC-0304](RFC-0304_VNCommittees.md). The committees
form a fixed set of replicas, at the very least from checkpoint to checkpoint, and will typically be limited in size,
usually less than ten, and almost always under 100. _Note_: These numbers are highly speculative based on an intuitive
guess about the main use cases for Tari DAs, where we have
* many 1-3-sized committees where the asset issuer and the VN committee are the same entity,
* semi-decentralised assets of ±4-10 where speed trumps censorship-resistance,
* a small number of 50-100 VNs where censorship-resistance trumps speed.

Because nodes cannot join and leave the committees at will, robust yet slow and expensive consensus approaches such as
Nakamoto consensus can be dropped in favour of something more performant.

There is a good survey of consensus mechanisms on
[Tari Labs University](https://tlu.tarilabs.com/consensus-mechanisms/consensus-mechanisms.html).

From the point of view of a DAN committee, the ideal consensus algorithm is one that
1. Allows a high number of transactions per second, and doesn't have unnecessary pauses (i.e. a partially synchronous or
   asynchronous model).
2. Is Byzantine Fault tolerant.
3. Is relatively efficient from a network communication point of view (number of messages passed per state agreement).
4. Is relatively simple to implement (to reduce the bug and vulnerability surface in implementations).

A summary of some of the most well-known BFT algorithms is presented in
[this table](https://tlu.tarilabs.com/consensus-mechanisms/BFT-consensus-mechanisms-applications/MainReport.html#summary-of-findings).

A close reading of the algorithms presented suggest that [LinBFT](https://arxiv.org/pdf/1807.01829.pdf), which is based
on [HotStuff] BFT provide the best trade-offs for the goals that a DAN committee is trying to achieve:
1. The algorithm is optimistic, i.e. as soon as quorum is reached on a particular state, the committee can move onto the
   next one. There is no need to wait for the "timeout" period as we do in e.g. Tendermint. This allows instructions to
   be executed almost as quickly as they are received.
2. The algorithm is efficient in communication, requiring O(n) messages per state agreement in most practical cases.
   This is compared to e.g. PBFT which requires O(n<sup>4</sup>) messages.
3. The algorithm is modular and relatively simple to implement.

Potential drawbacks to using HotStuff include:
1. Each round required the election of a _leader_. Having a leader dramatically simplifies the consensus algorithm; it
   allows a linear number of messages to be sent between the leader and the other replicas in order to agree on the
   current state; and it allows a strict ordering to be established on instructions without having to resort to e.g.
   proof of work. However, if the choice of leader is deterministic, attackers can identify and potentially DDOS the
   leader for a given round, causing the algorithm to time out. There are ways to mitigate this attack for a _specific
   round_, as suggested in the LinBFT paper, such as using Verifiable Random Functions, but DDOSing a single replica
   means that, on average, the algorithm will time out every 1/n rounds.
2. The attack described above only pauses progress in Hotstuff for the timeout period. In similar protocols, e.g.
   Tendermint it can be shown to [delay progress indefinitely](https://arxiv.org/pdf/1803.05069).

Given these trade-offs, there is strong evidence to suggest that [HotStuff] BFT, when implemented on the Tari DAN will
provide BFT security guarantees with liveness performance in the sub-second scale and throughput on the order of
thousands of instructions per second, if the benchmarks presented in the [HotStuff] paper are representative.

### Implementation

The [HotStuff] BFT algorithm provides a detailed description of the consensus algorithm. Only a summary of it is
presented here. To reduce confusion, we adopt the HotStuff nomenclature to describe state changes, rounds and function
names where appropriate.

Every proposed state change, as a result of replicas receiving instructions from clients is called a _view_. There is a
[function that every node can call](#leader-selection) that will tell it which replica will be the _leader_ for a given
view. Every view goes through three phases (`Prepare`, `PreCommit`, `Commit`) before final consensus is reached. Once a
view reaches the `Commit` phase, it is finalised and will never be reversed.

As part of their normal operation, every replica broadcasts [instructions] it receives for its contract to its peers.
These instructions are stored in a replica's instruction mempool.

When the [leader selection](#leader-selection) function designates a replica as leader for the next view, it will try
and execute _all_ the instructions it currently has in its mempool to update the state for the next view. Following this
it compiles a tuple of <_valid-instructions_, _rejected-instructions_, _new-state_>. This tuple represents the `CMD`
structure described in [HotStuff].

In parallel with this, the leader expects a set of `NewView` messages from the other replicas, indicating that the other
replicas know that this replica is the leader for the next view.

Once a super-majority of these messages have been received, the leader composes a proposal for the next state by adding
a new node to the state history graph (I'm calling it a state history graph to avoid naming confusion, but it's really a
blockchain). It composes a message containing the new proposal, and broadcasts it to the other replicas.

Replicas, on receipt of the proposal, decide whether the proposal is valid, both from a protocol point of view (i.e. did
the leader provide a well-formed proposal) as well as whether they agree on the new state (e.g. by executing the
instructions as given and comparing the resulting state with that of the proposal). If there is agreement, they vote on
the proposal by signing it, and sending their partial signature back to the leader.

When the leader has received a super-majority of votes, it sends a message back to the replicas with the (aggregated)
set of signatures.

Replicas can validate this signature and provide another partial signature indicating that they've received the first
aggregated signature for the proposal.

At this point, all replicas know that enough other replicas have received the proposal and are in agreement that it is
valid.

In Tendermint, replicas would now wait for the timeout period to make sure that the proposal wasn't going to be
superseded before finalising the proposal. But there is an attack described in the [HotStuff] paper that could stall
progress at this point.

The HotStuff protocol prevents this by having a final round of confirmations with the leader. This prevents the stalling
attack and _also_ lets replicas finalise the proposal _immediately_ on receipt of the final confirmation from the
leader. This lets HotStuff proceed at "network" speed, rather than with a heartbeat dictated by the BFT synchronicity
parameter.

Although there are 4 round trips of communication between replicas and the leader, the number of messages sent are O(n).
It's also possible to stagger and layer these rounds on top of each other, so that there are always four voting rounds
happening simultaneously, rather than waiting for one to complete in its entirety before moving onto the next one.
Further details are given in the [HotStuff] paper.

#### Forks and byzantine failures

The summary of the HotStuff protocol given above describes the "Happy Path", when there are no breakdowns in
communication, or when the leader is an honest node. In cases where the leader is unavailable, the protocol will time
out, the current view will be abandoned, and all replicas will move onto the next view.

If a leader is not honest, replicas will reject its proposal, and move onto the next view.

If there is a temporary network partition, the chain may fork (up to a depth of three), but the protocol guarantees
safety via the voting mechanism, and the chain will reconcile once the partition resolves.

#### Leader selection

[HotStuff] leaves the leader selection algorithm to the application. Usually, a round-robin approach is suggested for
its simplicity. However, this requires the replicas to _reliably_ self-order themselves before starting with SMR, which
is a non-trivial exercise in byzantine conditions.

For Tari DAN committees, the following algorithm is proposed:
1. Every replica knows the [Node ID] of every other replica in the committee.
2. For a given _view number_, the Node ID with the closest XOR distance to the hash of the _view number_ will be the
   leader for that view, where the hash function provides a uniformly random value of the same length as the Node ID.


#### Quorum Certificate

A Quorum certificate, or QC is proof that a super-majority of replicas have agreed on a given state. In particular, a QC
consists of
* The type of QC (depending on the phase in which the HotStuff pipeline the QC was signed),
* The _view number_ for the QC
* A reference to the node in the state tree being ratified,
* A signature from a super-majority of replicas.

### Tari-specific considerations

As soon as a state is finalised, replicas can inform clients as to the result of instructions they have submitted (in
the affirmative or negative). Given that HotStuff proceeds optimistically, and finalisation happens after 4 rounds of
communication, it's anticipated that clients can receive a final response from the validator committee in under 500 ms
for reasonably-sized committees (this value is speculation at present and will be updated once exploratory experiments
have been carried out).

The Tari communication platform was designed to handle peer-to-peer messaging of the type described in [HotStuff], and
therefore the protocol implementation should be relatively straightforward.

The "state" agreed upon by the VN committee will not only include the smart-contract state, but instruction fee
allocations and periodic checkpoints onto the base layer.

Checkpoints onto the base layer achieve several goals:
* Offers a proof-of-work backstop against "evil committees". Without proof of work, there's nothing stopping an evil
  committee (one that controls a super-majority of replicas) from rewriting history. Proof-of-work is the only reliable
  and practical method that currently exists to make it expensive to change the history of a chain of records. Tari
  gives us a "best of both worlds" scenario wherein an evil committee would have to rewrite the base layer history
  (which _does_ use proof-of-work) before they could rewrite the digital asset history (which does not).
* They allow the asset issuer to authorise changes in the VN committee replica set.
* It allows asset owners to have an immutable proof of asset ownership long after the VN committee has dissolved after
  the useful end-of-life of a smart contract.
* Provides a means for an asset issuer to resurrect a smart contract long after the original contract has terminated.

When Validator Nodes run smart contracts, they should be run in a separate thread so that if a smart contract crashes,
it does not bring the consensus algorithm down with it.

Furthermore, VNs should be able to quickly revert state to at least four views back in order to handle temporary forks.
Nodes should also be able to initialise/resume a smart contract (e.g. from a crash) given a state, view number, and view
history.

This implies that VNs, in addition to passing around HotStuff BFT messages, will expose additional APIs in order to
* allow lagging replicas to catch up in the execution state.
* Provide information to (authorised) clients regarding the most recent finalised state of the smart contract via a
  read-only API.
* Accept smart-contract instructions from clients and forward these onto the other replicas in the VN committee.

[committees]: Glossary.md#committee
[Node ID]: Glossary.md#node-id
[instructions]: Glossary.md#instructions
[HotStuff]: https://arxiv.org/pdf/1803.05069 "Hotstuff BFT"