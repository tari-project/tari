# RFC-0312/DANSpecification

## High level Digital Asset Network Specification

![status: draft](theme/images/status-draft.svg)

**Maintainer(s)**: [Cayle Sharrock](https://github.com/CjS77)

# Licence

[The 3-Clause BSD Licence](https://opensource.org/licenses/BSD-3-Clause).

Copyright 2022 The Tari Development Community

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
SPECIAL, EXEMPLARY OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES;
LOSS OF USE, DATA OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
CONTRACT, STRICT LIABILITY OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

# Language

The keywords "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED",
"NOT RECOMMENDED", "MAY" and "OPTIONAL" in this document are to be interpreted as described in
[BCP 14](https://tools.ietf.org/html/bcp14) (covering RFC2119 and RFC8174) when, and only when, they appear in all
capitals, as shown here.

# Disclaimer

This document and its content are intended for information purposes only and may be subject to change or update without
notice.

This document may include preliminary concepts that may or may not be in the process of being developed by the Tari
community. The release of this document is intended solely for review and discussion by the community of the
technological merits of the potential system outlined herein.

# Goals

This document describes the high-level, or informal specification for how digital assets are created, managed, secured,
and wound- down on the Tari digital asset network (DAN).

The document covers, among other things:

* The relationship of side-chains to digital assets and contract,
* Required characteristics of side-chains,
* Peg-in and peg-out mechanisms,
* Digital asset template minimum requirements,
* Validator node requirements,
* Checkpoint and refund mechanisms,
* Failure mode strategies.

This RFC covers a lot of ground. Therefore the intent is not to provide a detailed, code-ready specification for the
entire DAN infrastructure; those are left to other RFCs; but to establish a foundation onto which the rest of the DAN
specifications can be built.

This RFC supersedes and deprecates several older RFCs:

- [RFC-0300: Digital Assets Network](RFCD-0300_DAN.md)
- [RFC-0301: Namespace Registration](RFC-0301_NamespaceRegistration.md)
- [RFC-0302: Validator Nodes](RFCD-0302_ValidatorNodes.md)
- [RFC-0304: Validator Node committee selection](RFCD-0304_VNCommittees.md)
- [RFC-0345: Asset Life cycle](RFC-0345_AssetLifeCycle.md)

Several RFC documents are in the process of being revised in order to fit into this proposed framework:

* [RFC-0300: The Digital Assets Network](RFCD-0300_DAN.md)
* [RFC-0340: Validator Node Consensus](RFC-0340_VNConsensusOverview.md)

## Motivation

There are many ways to skin a cat. The philosophy guiding the approach in the RFC is one that permits scaling of the
network to handle in the region of **1 billion messages per day** (network-wide) and
**1 million digital assets** with **near real-time user experience** on asset state retrieval, updating and transfer, on
a sufficiently decentralised and private basis.

The definition of _sufficient_ here is subjective, and part of the design philosophy of Tari is that we leave it up to
the user to determine what that means, keeping in mind that there is always a trade-off between decentralisation,
performance, and cost.

For some assets, decentralisation and censorship resistance will be paramount, and users will be willing to live with a
more laggy experience. Gamers in a Web 3.0-MMORPG on the other hand, want cheap, fast transactions with verifiable
ownership, and therefore will generally need to sacrifice decentralisation for that.

The goal of the DAN is for asset issuers to be able to configure the side-chain for their project to suit their
particular needs.

# Description

There are several key actors that participate in Tari Digital Asset Network:

* A tari [contract] is a piece of code that establishes the relationship and rules of engagement between one or more
  digital assets. This includes ownership rules, transfer rules and state change rules.
* The [Asset issuer] is the entity that defines a contract and brings it into existence.
* [Validator node]s manage the contract on behalf of the asset issuer by executing instructions on a Tari [side-chain].
* [Users] interact with contracts and may own, transfer or execute state change instructions against the contract by
  submitting instructions via the Tari [comms network] to the relevant validator node committee.

## The role of the Layer 1 base chain

The Tari Overview RFC describes [the role of the base layer]. In summary, the base layer

* maintains the integrity of the Tari cryptocurrency token, and
* maintains registers of the side-chains,
* and facilitates the version control and reproducible execution environments for contract templates.

It does not know about or care about what happens in the side chains as long as the Tari consensus, side-chain and
validator node rules are kept.

It is helpful to view the base layer blocks and transactions as an immutable, append-only document which allows us to
model the tables and foreign relationships of a traditional database. The rows are represented by the UTXOs and we can
infer which table the row belongs to by inspecting the output features of the UTXO.

Whereas a standard RDMS manages access control and permissions via policy, we must also take care to ensure proper
access control via consensus rules, lock scripts, covenants, signatures and kernels.

## Top-level requirements for side-chains

The guiding principle of Tari contracts are that they are managed on a dedicated side-chain. One side-chain, one
contract. Other RFCs will discuss ways to overcome the apparent limitations this rule implies, including inter-contract
interactions and asset hibernation.

### Asset issuer <-> Validator node agreements

The fundamental relationship of Tari contracts is between the asset issuer and the validator node(s) that manage the
contract's side-chain. This relationship is somewhat adversarial by nature: Issuers want high quality service at the
lowest possible price; Validators want to be compensated for their services and under some circumstances may want to
cheat on contracts for their own gain.

Tari seeks to address this in the lightest way possible by requiring the absolute minimum in terms of base layer
governance while providing options for side-chain governance that suits the needs of the parties involved.

For example, an asset issuer that wants to issue a highly decentralised, censorship-resistant _high-value_ contract on a
side-chain would likely seek to recruit dozens of validator nodes and run a proof-of-stake consensus model with a
confidential asset specification.

In contrast, an asset issuer that wants to participate in the Tari ecosystem, but is not interested in decentralisation
could run their own validator node; with no consensus, or staking, or validator node compensation contracts -- these
would be unnecessary; and provide a high performance, real-time contract. Games with realistic embedded economics would
follow this model, as well as early on in the transition from tradFi to deFi.

A set of Validator nodes that manage the same contract is called the _validator node committee_ for the contract.

### The Asset issuer
[asset issuer]: #asset_issuer

The asset issuer, otherwise known as the contract owner, is the entity that publishes
a [contract definition transaction].

The [contract definition transaction] defines the "what" of the contract. It specifies the complete specification of the
code that will run, the execution environment it must be run under, as well as the initialisation parameters for all the
contract template constructors.

The contract definition allows validator nodes to be confident that they are running a byte-for-byte equivalent code
base with the exact same interpretation of that code as its peers without having to collaborate with any other nodes to
confirm this.

In most cases, a contract definition will comprise several well-reviewed and secure _templates_ to define the operation
of the contract.

The asset issuer will also draft and publish the [contract constitution]. The constitution defines _how_ a contract is
run, and defines the conditions under which the terms of the constitution can be changed.

### The role of validator nodes
[validator node]: #the-role-of-validator-nodes

* Validator nodes SHOULD diligently and accurately [process all instructions] related to the contract.
* The committee SHOULD reach consensus on every instruction related to the contract. This specification does NOT dictate
  how this consensus is reached. If the committee contains one member, then consensus is trivial, and does not require
  any complicated consensus algorithms. A standard web-based application stack will suffice in most cases. Larger
  committees can choose from any manner of consensus algorithms, including PBFT, HotStuff, proof-of-stake or
  proof-of-work.

**OPEN QUESTION**: The asset issuer has no in-band way to know how the VNs are reaching consensus. Even out-of-band,
there could be one server and a bunch of proxies that merely relay messages. Only proof of work (because it is
permissionless) and proof of stake (maybe?) work around this problem.

* TODO - research how Polygon and other multichain networks solve this problem.

The Tari base layer does not get involved in governance issues beyond those mechanics that are defined in contract
constitutions. However, many asset issuers may want to include mechanisms that, for example, require a Tari stake to act
as a validator node. Validator nodes may also desire a compensation mechanism so that they get paid for managing the
contract. These mechanisms form part of the contract itself, and are opaque to the machinery of the base layer,
side-chain and associated peg transactions.

Validator nodes MAY have to stake Tari for each contract they validate. Asset issuers will determine the nature and
amount of stake required as part of the [contract constitution]. The contract stake is variable on a
contract-to-contract basis so that an efficient market between asset issuers and validator nodes can develop. This
market is not defined on the Tari blockchain at all and would be best implemented as a DAO on the DAN itself.

Similarly, it has been suggested in the past that Validator Nodes should post hardware benchmarks when registering. The
problem with this requirement is that it is fairly trivial to game. We cannot enforce that the machine that posted the
benchmark is the same as the one that is running validations.

A better approach is to leave this to the market. A reputation contract can be built, on Tari, of course, that
periodically and randomly asks Validator Nodes to perform cryptographically signed benchmarks in exchange for
performance certificates. Nodes can voluntarily sign up for such a service and use the certificates as a form of
credential. Nodes that do not sign up may have trouble finding contracts to validate and might have to lower their price
to get work.

Tari contracts are template-based, and so many contracts may wish to include [contract template]s that add any or all of
the following governance functions to the side-chain contract:

* Validator node staking.
* Validator node slashing.
* A Validator node proof-of-participation certificate template. Poorly performing validator nodes may receive reduced
  compensation, be fined, or even ejected from the committee at a checkpoint.
* A fee model template. The asset issuer could provide a guaranteed pool of funds from which the committee will be paid
  at every checkpoint.

This list is far from complete, but should convey the idea that:

* Tari contracts SHOULD be highly modular and composable, with each template performing exactly ONE highly specific
  task, and doing it very well.
* The base layer and peg transactions know the absolute minimum about the assets on the chain. However, they provide all
  the information necessary for the contract templates and side-chains to function efficiently.

## The contract lifecycle
[contract lifecycle]: #the-contract-lifecycle "The contract lifecycle"

Every contract MUST be governed by one, and only one, Tari [side-chain]. A contract MAY define one or more digital
assets. This contract can be very simple or highly complex.

The lifecycle of a contract proceeds via these steps:

1. The asset issuer publishes a [contract definition transaction].
2. The asset issuer publishes a [contract constitution] transaction.
3. Once this transaction is published, we enter the [acceptance period].
4. Each validator node that will be managing the contract publishes a [contract acceptance transaction]. The group of
   validator nodes that manages the contract is called the Validator Node Committee (VNC).
5. Once the [acceptance period] has expired, the [side-chain initialization period] begins.
6. The VNC jointly publishes a [side-chain initialization] transaction.
7. At this point, the contract is considered live, and users can safely interact with the contract on the side-chain.
   Technically, users do not have to wait until this point. The VNC COULD start processing transactions
   _optimistically_ as soon as the constitution is published, and print the zero-th and first checkpoints once they are
   mined on the base layer. However, this is not generally recommended.
8. The VNC periodically publishes a [checkpoint] transaction.
9. Failure to do so can lead to the contract being [abandoned].
10. The VNC MAY shut the contract down by publishing a [dissolution] transaction.

The following sections will discuss each of these steps in more detail.

## Contract instantiation
[contract instantiation]: #contract-instantiation

Steps 1 - 6 in the [contract lifecycle] are part of the [contract instantiation] process. Instantiation is a multi-step
process and is ideally represented as a finite-state machine that reacts to transactions published on chain that contain
outputs containing specific output features. The combination of output features and FSM allows nodes to accurately track
the progress of potentially thousands of contracts in a safe and decentralised manner.

### The contract definition transaction
[contract definition transaction]: #the-contract-definition-transaction

It bears repeating that every contract is governed by one, and only one, Tari [side-chain]. A contract MAY define one or
more digital assets. These assets' behaviour is captured in templates and are highly composable. This allows the
contract to be very simple or highly complex, and be handled with the same contract handling machinery.

<note :tip>
The contract definition transaction defines the "what" of the digital asset set that will be created.
</note>

* Every contract MUST be registered on the base layer.
* Contracts MUST be registered by publishing a `contract definition` transaction.
* Asset issuers MUST stake a small amount of Tari in order to publish a new contract.
* Exactly ONE output MUST have a `ContractSpecification` output feature.
* The contract specification UTXO MUST include a covenant that only permits it to be spent to a
  new `ContractSpecification` UTXO (when transferring ownership of a contract), or as an unencumbered UTXO in
  a `ContractDeregistration` transaction.

Note: The latter is desirable because it tidies up the UTXO set. But this transaction MUST NOT be published before
contract has been dissolved (see [contract dissolution]).

* The  `ContractSpecification` UTXO MUST hold at least the `MINIMUM_OWNER_COLLATERAL` in Tari. The amount is hard-coded
  into consensus rules and is a nominal amount to prevent spam, and encourages asset owners to tidy up after themselves
  if a contract winds down. Initially, `MINIMUM_OWNER_COLLATERAL` is set at 200 Tari, but MAY be changed across network
  upgrades.

**Implementation note:**
Assuming the collateral is represented by the UTXO commitment $C = kG + vH$, the minimum requirement is verified by
having the range-proof commit to $(k, v - v_\mathrm{min})$ rather than the usual $(k, v)$. Note that this change
requires us to modify the
`TransactionOutput` definition to include a `minimum_value_commitment` field, defaulting to zero, to capture this extra
information.

* The `ContractSpecification`UTXO MUST also include:
    * The contract description,
    * the asset issuer record
    * the contract definition, as described below.

#### Contract description

The contract description is a simple metadata record that provides context for the contract. The record includes:

* The contract id -- `<u256 hash>`. This is immutable for the life of the contract and is calculated as
  `H(contract_name || contract specification hash || Initial data hash || Runtime data hash)`.
* A contract name -- `utf-8 char[32]`(UTF-8 string) 32 bytes. This is for informational purposes only, so it shouldn't
  be too long, but not too short that it's not useful (this isn't DOS 3.1 after all). 32 bytes is the same length as a
  public key or hash, so feels like a reasonable compromise.

#### Asset issuer record
The asset issuer record identifies the [asset issuer] as the initial owner and publisher of the contract. The following
fields are required:

* the asset issuer's public key, also known as the owner public key, `<PublicKey>`.

#### Contract definition
The following information must be captured as part of the `contract definition` in the `ContractSpecification`UTXO of
the contract definition transaction:

* the full contract specification in a compact serialised format,
* the initialisation arguments for the contract, in a compact serialisation format,
* the runtime specification.

This data tells validator nodes _exactly_ what code will be running, and the data needed to initialise that code.

Asset templates will have a strictly defined interface that includes a constructor, or initialisation method. The
parameters that these constructors accept is what determines the initial data.

The runtime specification includes, for example, the version of the runtime and any meta-parameters that the runtime
accepts.

These three pieces of data are _necessary_ AND _sufficient_ to enable _any_ validator node to start running the contract
and execute instructions on it, knowing that any other validator node running the same contract will determine _exactly_
the same state changes for every instruction it receives.

### The contract constitution
[contract constitution]: #the-contract-constitution

Following the [contract definition transaction],the asset issuer MUST publish a [contract constitution] transaction in
order for the contract initialisation process to proceed.

This transaction defines the "how" and "who" of the digital asset's management.

It contains the "contract terms" for the management of the contract.

Exactly ONE UTXO MUST include the `ContractConstitution` output feature flag. The contract constitution UTXO contains
the following:

* It MUST include the contract id. The contract definition transaction SHOULD be mined prior to publication of the
  constitution transaction, but it strictly is not necessary if VNs are able to access the contract specification in
  some other way.
* It MUST include a list of public keys of the proposed VNC;
* It MUST include an expiry timestamp before which all VNs must sign and agree to these terms (the [acceptance period]);
* It MAY include quorum conditions for acceptance of this proposal (default to 100% of VN signatures required);
* If the conditions will unequivocally pass, the acceptance period MAY be shortcut.
* There MAY be an initial reward that is paid to the VN committee when the UTXO is spent. This reward is simply included
  in the value of the `ContractConstitution` UTXO.
* The UTXO MUST only be spendable by a multisig of the quorum of VNs performing [side-chain] initialisation. (e.g. a 3
  of 5 threshold signature).
* It MUST include the side-chain metadata record:
    * The consensus algorithm to be used
    * checkpoint quorum requirements
* It MUST include the following Checkpoint Parameters Record
    * minimum checkpoint frequency,
    * committee change rules. (e.g. asset issuer must sign, or a quorum of VNC members, or a whitelist of keys).
* It MAY include a `RequirementsForConstitutionChange` record. It omitted, the checkpoint parameters and side-chain
  metadata records are immutable via covenant.
    * How and when the Checkpoint Parameters record can change.
    * How and when the side-chain metadata record can change
* It SHOULD include a list of emergency public keys that have signing power if the contract is [abandoned].

If both the [acceptance period] and [side-chain initialization period] elapses without quorum, the asset owner MAY spend
the`ContractConstitution` UTXO back to himself to recover his funds.

In this case, the asset issuer MAY try and publish a new contract constitution.

#### Contract constitutions for proof-of-work side-chains

Miners are joining and leaving PoW chains all the time. It is impractical to require a full constitution change cycle to
execute every time this happens, the chain would never make progress!

To work around this, the constitution actually defines a set of proxy- or observer-nodes that perform the role of
running a full node on the side chain and publishing the required [checkpoint transaction]s onto the Tari base chain.
The observer node(s) are then technically the VNC. Issuers could place additional safeguards in the contract definition
and constitution to keep the VNC honest. Conceivably, even Monero or Bitcoin itself could be attached as a side-chain to
Tari in this manner.

### The contract acceptance transaction
[contract acceptance transaction]: #the-contract-acceptance-transaction

[acceptance period]: #the-contract-acceptance-transaction "The contract acceptance transaction"

The entities that are nominated as members of a VNC for a new contract MUST cryptographically [acknowledge and agree] to
manage the contract. This happens by virtue of the [contract acceptance transaction]s.

* Each potential VNC member MUST publish a [contract acceptance transaction] committing the required stake. The UTXO is
  also an explicit agreement to manage the contract.
* Exactly ONE UTXO MUST have the output feature `ContractAcceptance`.
* The UTXO MUST contain a time lock, that prevents the VN spending the UTXO before the [acceptance period]
  + [side-chain initialization period] has lapsed.
* The output MUST include the contract id.

A contract acceptance transaction MUST be rejected if

* contract id does not exist (the contract definition has not been mined)
* the signing public key was not nominated in the relevant contract constitution
* the deposit is insufficient

### The side-chain initialization period
[side-chain initialization period]: #the-side-chain-initialization-period "The side-chain initialization period"

Once the [acceptance period] has expired, [side-chain initialization period] begins.

At this point, VNs that have accepted the contract must

* allocate resources
* Setup whatever is needed to run the contract
* Set up consensus with their peer VNs (e.g. hotstuff)
* Intialise the contract and run the constructors
* Reach consensus on the initial state.
* Prepare the [side-chain initialization] transaction.

all before the [side-chain initialization] period expires.

### The side-chain initialization transaction
[side-chain initialization]: #the-side-chain-initialization-transaction "The side-chain initialization transaction"

[side-chain]:#the-side-chain-initialization-transaction "The side-chain initialization transaction"

Side-chains MUST be marked as initiated by virtue of a [side-chain initialization] transaction.

* Once the [acceptance period] has expired, [side-chain initialization period] begins.
* At this point, there MUST be a quorum of acceptance transactions from validator nodes.
* The validator node committee MUST collaborate to produce, sign and broadcast the initialisation transaction by
  spending the initial Contract Constitution transaction into the zero-th [checkpoint] transaction.
* The initialisation transaction MUST spend all the [contract acceptance transactions] for the contract.
* Base layer consensus MUST confirm that the spending rules and covenants have been observed, and that the checkpoint
  contains the correct covenants and output flags.
* There is a minimum [side-chain deposit] that MUST be included in the peg-in UTXO. A single aggregated UTXO containing
  at least $$ m D $$ Tari, where _m_ is the number of VNs and _D_ is the deposit required.
* The initial reward, if any, MAY be spent to any number of unencumbered outputs. In practice the VNC will have
  negotiated how to spend this, and so we can assume that consensus has been reached on how to spend the initial reward.
  Therefore the base layer places no additional restrictions on those funds.
* This transaction also acts as the zero-th checkpoint for the contract. As such, it requires all the checkpoint
  information.
* The state commitment is the merklish root of the state after running the code initialisation using the [initial data]
  provided in the [contract definition transaction].

## Contract execution
The goal of the DAN is to allow many, if not millions, of instructions to be processed on the side-chain with little or
no impact on the size of the base layer.

The only requirements that the base layer will enforce during contract execution are those specified in the contract
constitution.

The base layer will check and enforce these requirements at [checkpoint]s.

### Checkpoint transactions
[checkpoint]: #checkpoint-transactions

The roles of the checkpoint transaction:

* Present proof of liveness
* Allows authorised entities to make changes to the committee
* Summarise contract state
* Summarise contract logs / events

**Implementation Note:** In the discussion of Tari account contract templates below, we need a mechanism for proving
that the side-chain state corresponds to what someone is claiming with respect to a valid base layer transaction. But
since our policy is one that the base layer never knows anything about what goes on in side-chains, this poses a
challenge. One possible solution to this would be to add a `MERKLE_PROOF` opcode to TariScript that could validate a
base layer transaction based on a checkpoint merkle root combined with a merkle proof that a VNC has given to a user.

Validator node committees MUST periodically sign and broadcast a [checkpoint] transaction.

The transaction signature MUST satisfy the requirements laid out for checkpoint transactions defined in
the [contract constitution].

* The checkpoint transaction MUST spend the previous checkpoint transaction for this contract. Consensus will guarantee
  that only one checkpoint UTXO exists for every contract on the base layer. This is guaranteed by virtue of a covenant.
  The contract id must equal the contract id of the checkpoint being spent.
* The checkpoint transaction MUST contain exactly ONE UTXO with the `Checkpoint` output feature.

The `Checkpoint`output feature adheres to the following:

* It MUST reference the contract id.
* It MUST contain a commitment to the current contract state. This is typically some sort of Merklish root.
* It MAY have a URI to off-chain state or merkle tree
* It MUST contain a checkpoint number, strictly increasing by 1 from the previous checkpoint.
* It MUST strictly copy over the constitution rules from the previous checkpoint, OR
* It MUST contain valid signatures according to the constitution allowing the rules to be changed, along with the
  relevant parts of the [contract constitution] change pipeline.

If a valid checkpoint is not posted within the maximum allowed timeframe, the contract is [abandoned]. This COULD lead
to penalties and stake slashing if enabled within the contract specification.

### Changes to the constitution
[contract constitution change]: #changes-to-the-constitution

Any changes to the [contract constitution] MUST happen at checkpoints. This also refers to any changes to the VNC.

* The rules over how members are added or removed are defined in the [contract constitution]. It may be that the VNC has
  autonomy over these changes, or that the asset issuer must approve changes, or some other authorization mechanism.
* At the minimum, there's a proposal step, a validation step, an acceptance step, and an activation step. Therefore
  changes take place over at least a 4-checkpoint time span.
* If a VN leaves a committee their [side-chain deposit] MAY be refunded to them.
* If a new VN joins the committee they must provide the [side-chain deposit] at their activation step.
* In the proposal step, any authorised VNC change proposer, as defined in the [contract constitution]

### Contract abandonment
[abandoned]: #contract-abandonment "Contract abandonment"

If a contract misses one or more checkpoints, nodes can mark it as `abandoned`. This is not formally marked on the
blockchain, (since something was NOT done on-chain), but nodes will be able to test for abandoned state.

The [contract constitution] SHOULD provide a set of emergency pubkeys that are able to

* perform a peg-out
* do all governancy things
* rescue funds and state

Implementation note: We could add an `IS_ABANDONED` opcode (sugar for height since last checkpoint) to test for
abandonment.

If a contract is abandoned, the emergency key MAY spend the last checkpoint into a `QUARANTINED` state. A contract MUST
stay in `QUARANTINED` state for at least one month.

The contract can leave the quarantined state in one of two ways:

* The current VNC MAY reinstate the contract operation by publishing the missing checkpoints, and committing to any
  remedial actions as specified in the [contract constitution], e.g. paying a fine, etc.
* The quarantine period lapses, at which point the emergency key holder(s) have full administrative power over the
  contract. So they can unilaterally establish a brand new VNC, peg-out and shut down the contract, or whatever.

## Contract dissolution
[dissolution]: #contract-dissolution "Contract dissolution"

## Contract templates
[contract template]: #contract-templates

### Template code registration and versioning

The code template implementations MUST be registered on the base layer.

The reason for this is that it allows Validator Nodes to know unequivocally that they are all running the same code and
can expect the same output for the same input.

Template registration also allows us to implement a secure and trust-minimised upgrade mechanism for templates.

Potentially, we could even introduce a mechanism wherein template developers get paid for people using their template.

Template registration UTXO would contain:

* A link to the code (git commit or IPFS)
* The type of code (source or binary blob)
* A hash of the source code / blob
* Version info.
* [Execution engine] requirements (similar to solc pragma)

There's a clear upgrade path, since there's a code-chain from one version of a contract template to the next.

## User account balance representation in side-chains
[contract user accounts]: #user-account-balance-representation-in-side-chains

Tari uses the UTXO model in its ledger accounting. On the other hand Tari side-chains SHOULD use an account-based system
to track balances and state.

The reasons for this are:

* An account-based approach leads to fewer outputs on peg-out transactions. There is roughly a 1:1 ratio of users to
  balances in an account-based system. On the other hand there are O(n) UTXOs in an output-based system where `n` are
  the number of transactions carried out on the side-chain. When a side-chain wants to shut down, they must record a new
  output on the base layer for every account or output (as the case may be) that they track in the peg-out transaction(
  s). It should be self-evident that account-based systems are far more scalable in the vast majority of use-cases.
* Following on from this, Accounts scale better for micro-payment applications, where hundreds or thousands of tiny
  payments flow between the same two parties.
* Many DAN applications will want to track state (such as NFTs) as well as currency balances. Account-based ledgers make
  this type of application far simpler.

### Pedersen commitments and account-based ledgers

Standard Pedersen commitments are essentially useless in account-based ledgers.

The reason being that since the spending keys would be common to all transactions involving a given account, it is
trivial to use the accounting rules to cancel out the `k.G` terms from transactions and to use a pre-image attack to
unblind all the values.

The specific protocol of user accounts in the side-chain is decided by the asset issuer.

Options include:

#### Fully trusted

In this configuration, the side-chain is controlled by a single validator node, perhaps a server running an RDMS. The
validator node has full visibility into the state of the side chain at all times. It may or may not share this state
with the public. If it does not, then the situation is analogous to current Web 2.0 server applications.

#### Decentralised and federated

In this configuration, a distributed set of validator nodes maintain the side-chain state. The set of nodes are fixed.
If consensus between nodes is achieved using a mechanism such as HotStuff BFT, very high throughputs can be achieved.

#### Decentralised and censorship resistant

In this configuration, the side-chain could itself be a proof-of-work blockchain. This offers maximum decentralisation
and censorship resistance. However, throughput will be lower.

#### Confidentiality

As mentioned above, Pedersen commitments are not suitable for account-based ledgers. However, the [Zether] protocol was
expressly designed to provide confidentiality in a smart-contract context. It can be combined with any of the above
schemes. Zether can also be [extended](https://github.com/ConsenSys/anonymous-zether) to provide privacy by including a
ring-signature scheme for transfers.

## Key template discussions

A majority of contracts will want to implement on or more of the following features:

* A financial bridge from the base layer and user accounts,
* A fee or compensation mechanism for the VNC,
* Inter-contract communications

These are complex topics and there are entire blockchain systems where this functionality is built into the fabric of
the design. Tari’s modular approach naturally means that the functionality will be delegated into templates and
instantiated where necessary and desired by [asset issuer]s.

This also means that Tari offers additional flexibility for issuers and users while the ecosystem is better positioned
to respond to changes in demand and new smart contract patterns.

For this RFC, we limit the conversation to a very broad description of how the templates could be implemented, but will
leave specifics to RFCs that are more focussed on the topic.

### Funding, withdrawals and deposits

Deposits and withdrawals go via a smart contract template using the bridge model.

#### Very high level flow

1. Send Tari via One-sided payment to an address defined by the template. (Could have a `DEPOSIT` output feature if
   required)
2. The VNC sees this, and then issues / prints / mints the equivalent value on side-chain according to the side-chain
   protocol.
3. Equivalent coins change hands many times. The account template maintains an accurate balance of all users’ accounts,
   with the VNC reaching consensus on value transfer instructions according to the consensus algorithm in force.
4. A User requests a withdrawal.
5. The VNC debits the user’s account and "burns" equivalent coins on the side chain.
6. The VNC broadcasts a standard one-side Tari transaction to the user’s benefit.
7. Optionally, the template functionality facilitating proofs of reserve, i.e. that locked funds are of equivalent value
   to minted funds.

Note that this model is not trustless from a base-layer point of view. Users are trusting the side chain, and VNC to not
steal their funds. Therefore one may want to encourage the deployment of PoW or PoS side-chains when executing contracts
that handle large amounts of value.

##### Possible variants

* Users deposit and get a refund transaction to hold onto.
* The refund tx gets updated every time the balance changes. ala Lightning.
* Proof of burn tied to proof of spend.
* Atomic swaps to force issue of token on side-chain in (1.) above.

We could implement any/all of these variants in different templates.

### Validator node fees

2 Template models:

- Model A - Centrally funded
- Fees are drawn from a single account (typically funded by asset issuer)
- Eligible instructions are defined in the template constructor.
- Model B - User funded
- Requires an account template
- Fees are supplied with an instruction
- Eligible instructions are defined in the template constructor.
- Instructions that are not covered by the model MAY be rejected by the VNC

### Validator Node Instructions
[process all instructions]: #validator-node-instructions

What does an instruction look like? Note: Solana instructions contain

- ProgramId
- Vec of accounts that the instruction will interact with (plus whether they're mutable and have signer auth)
- a blob that the program will deserialise. So, no inherently accessible API

Requires:

- Contract ID
- Vec of method calls: (this is different to how Solana does it/ Maybe some discussion on pros&cons is worthwhile. If we
  go WASM, the API is available via reflection)
    - Method ID (template::method)
    - Method arguments
- Authorization
    - signed token-based (Macaroons / JWTish)

Now the VNs have everything they need to execute the Instruction. They execute the instruction. The update the state
tree. Return of the call is a "diff" of some sort, which gets appended to the "OP Log" document, and the new state root
hash.

The VNC SHOULD reach consensus on this result.

Then you move onto the next instruction.

* Where do instructions get submitted?

    * The [peg-in transaction] contains the pubkeys of each member of the VNC; or a checkpoint transaction.
    * ergo, a client app knows the pubkeys of the VNC at all times.
    * A client can send an Instruction to ANY VNC member via comms
* VNs MUST maintain a mempool of instructions
* VNs SHOULD share instructions with its peer committee members
* Ordering of instructions.

    * (In Hotstuff) The leader selects the next instruction(s) to run.
    * The leader MAY batch instructions in a single consensus round.
    * For account-based side-chains, Instructions SHOULD contain a nonce??? (Might not be workable)
    * For account-based side-chains, Instructions COULD have a dependency field that forces ordering of selected
      instructions.
        * Potentially, an accumulator is a way to do this. An instruction provides a list of instruction hashes, and the
          instruction can be included ONLY IF ALL hashes have been recorded.
    * Instructions MUST not be executed more than once, even if resubmitted. Suggests some sort of salt/entropy/nonce so
      that the same execution steps could be run without being interpreted as the _same_ instruction. (e.g.
      micro-transactions).

### Inter-contract interactions

Possible routes for this:

#### Atomic transactions

- Provide a proof that a conditional instruction on one chain has been executed,
- Execute on this chain, which reveals some fact that the other chain can use to finalise the instruction on the other
  chain.
- Rolls back if 2nd party does not follow through.

Advantages:

- Does work.

Disadvantages

- Slow
- Need to get data from other chain.
- Might hold up entire chain for extended periods.

#### Observer protocol

Implement a set of APIs in a template for reading the event log from the VNC directly or query the "read-only" contract.

Pros:

- Fast
- Permissionless in one-way applications
- Can check that results are signed by the VNC quorum

Cons:

- Rely on contracts implementing the protocol
- Instructions that require both chains' state to update is harder using this method.

#### Micro-payments

##### Bundle accounts template into smart contract

* The bundled "wrapped" Tari is used in micropayments.
* Users top up or withdraw Tari into the micropayment accounts using a bride or one of the methods described above.

##### Async-await analogue

* Contract A is a digital assets contract.
* Contract B is a payments contract.
* A and Bob have a monetary account on B, and Bob wants access to the assets on A.
* Bob authorises A to debit his account on B for a certain amount / under certain conditions OR
* Bob authorises the invoice produced by A for a discrete payment.
* A submits a payment instruction to B to withdraw the amount, co-signed by Bob (or he did a pre-auth).
* A "awaits" the result of the payment, and once successful, releases the asset OR
* the instruction times out and Bob does not receive the asset and the instruction concludes.

Pros:

* Can work in general, not just micro-payments
* Can be fast.
* Doesn't block progress in the face of obstructive agents.

Cons:

* Complex (handling collusion, "proof-of-delivery")
* time-outs can lock up funds for long periods.
* Relies on chains publishing events.
* Contract B is a trusted party from the PoV of Bob / A (e.g. Bob & B collude to lie about account updates in order to
  defraud A)

# Change Log

* **06-04-2022**: First draft

[RFC-0001]: RFC-0001_overview.md
[the role of the base layer]: RFC-0001_overview.md#the-role-of-the-base-layer
[Zether]: https://eprint.iacr.org/2019/191.pdf
[comms network]: RFC-0172_PeerToPeerMessagingProtocol.md