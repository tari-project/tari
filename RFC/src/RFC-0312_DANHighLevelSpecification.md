# RFC-0312/DANSpecification

## High level Digital Asset Network Specification

![status: draft](theme/images/status-draft.svg)

**Maintainer(s)**: [Cayle Sharrock](https://github.com/CjS77)

# Licence

[The 3-Clause BSD Licence](https://opensource.org/licenses/BSD-3-Clause).

Copyright 2019 The Tari Development Community

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
community. The release of this document is intended solely for review and discussion by the community of the
technological merits of the potential system outlined herein.

## Goals

This document describes the high-level, or informal specification for how digital assets are created, managed, secured, and wound-
down on the Tari digital asset network (DAN).

The document covers, among other things:

* The relationship of side-chains to digital assets,
* Required characteristics of side-chains,
* Peg-in and peg-out mechanisms,
* Digital asset template minimum requirements,
* Validator node requirements,
* Checkpoint and refund mechanisms

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

### Motivation

There are many ways to skin a cat.
The philosophy guiding the approach in the RFC is one that permits
scaling of the network to handle in the region of **1 billion messages per day** (network-wide) and
**1 million digital assets** with **near real-time user experience** on asset state retrieval, updating and transfer,
on a sufficiently decentralised and private basis.

The definition of _sufficient_ here is subjective, and part of the design philosophy of Tari is that we leave it up to the
user to determine what that means, keeping in mind that there is always a trade-off between decentralisation, performance,
and cost.

For some assets, decentralisation and censorship resistance will be paramount, and users will be willing to live with a
more laggy experience. Gamers in a Web 3.0-MMORPG on the other hand, want cheap, fast transactions with verifiable ownership, and
therefore will generally need to sacrifice decentralisation for that.

The goal of the DAN is for asset issuers to be able to configure the side-chain for their project to suit their particular
needs.

## Description

There are several key actors that participate in Tari Digital Asset Network:

* A tari [contract] is a piece of code that establishes the relationship and rules of engagement between one or more
  digital assets. This includes ownership rules, transfer rules and state change rules.
* The [Asset issuer] is the entity that defines a contract and brings it into existence.
* [Validator node]s manage the contract on behalf of the asset issuer by executing instructions on a Tari [side-chain].
* [Users] interact with contracts and may own, transfer or execute state change instructions against the contract by
  submitting instructions via the Tari [comms network] to the relevant validator node committee.

### The role of the Layer 1 base chain

The Tari Overview RFC describes [the role of the base layer].
In summary, the base layer maintains the integrity of the Tari cryptocurrency token, and maintains registers of the side-chains,
validator nodes and contract templates.

It does not know about or care about what happens in the side chains as long as the Tari consensus, side-chain and
validator node rules are kept.

One can view the base layer blocks and transactions as an immutable, append-only document which is the physical manifestation
of a traditional database. The rows are represented by the UTXOs and we can infer which table the row belongs to by inspecting
the output features of the UTXO.

Whereas a standard RDMS manages access control and permissions via policy, we must also take care to ensure proper access control
via consensus rules, lock scripts, covenants, signatures and kernels.

### Top-level requirements for side-chains

The guiding principle of Tari contracts are that they are managed on a dedicated side-chain. One side-chain,
one contract.
Other RFCs will discuss ways to overcome the apparent limitations this rule implies, including inter-contract
interactions and asset hibernation.

#### Asset issuer <-> Validator node agreements

The fundamental relationship of Tari contracts is between the asset issuer and the validator node(s) that manage
the contract's side-chain. This relationship is somewhat adversarial by nature: Issuers want high quality service at
the lowest possible price; Validators want to be compensated for their services and under some circumstances may want
to cheat on contracts for their own gain.

Tari seeks to address this in the lightest way possible by requiring the absolute minimum in terms of base layer governance
while providing options for side-chain governance that suits the needs of the parties involved.

For example, an asset
issuer that wants to issue a highly decentralised, censorship-resistant _high-value_ contract on a side-chain would likely
seek to recruit dozens of validator nodes and run a proof-of-stake consensus model with a confidential asset specification.

In contrast, an asset issuer that wants to participate in the Tari ecosystem, but is not interested in decentralisation
could run their own validator node; with no consensus, or staking, or validator node compensation contracts -- these
would be unnecessary; and provide a high performance, real-time contract. Games with realistic embedded economics would
follow this model, as well as early on in the transition from tradFi to deFi.

A set of Validator nodes that manage the same contract is called the _validator node committee_ for the contract.

### The Asset issuer
[asset issuer]: #asset_issuer

The asset issuer, otherwise known as the contract owner, is the entity that publishes a [contract creation transaction].

* The asset issuer MAY transfer ownership of a contract to a new entity.
* The asset issuer MAY migrate a contract to a new version.

OPEN QUESTION: What rights are unquestionably given to the owner? Maybe the covenant should be somewhat flexible.
Contracts that do not allow the owner to deregister it will be more trustworthy than those that do.

#### The role of validator nodes

* Validator nodes SHOULD diligently and accurately [process all instructions] related to the contract.
* The committee SHOULD reach consensus on every instruction related to the contract. This specification does NOT dictate how this
  consensus is reached. If the committee contains one member, then consensus is trivial, and does not require any complicated
  consensus algorithms. A standard web-based application stack will suffice in most cases.
  Larger committees can choose from any manner of consensus algorithms, including PBFT, HotStuff, proof-of-stake or
  proof-of-work.

**OPEN QUESTION**: The asset issuer has no in-band way to know how the VNs are reaching consensus. Even out-of-band,
there could be one server and a bunch of proxies that merely relay messages. Only proof of work (because it is permissionless)
and proof of stake (maybe?) work around this problem. We need some sort of proof-of-uniqueness mechanism here... :thinking:

The Tari base layer does not get involved in governance issues. However, many asset issuers may want to include mechanisms
that, for example, require a Tari stake to act as a validator node. Validator nodes may also desire a compensation
mechanism so that they get paid for managing the contract. These mechanisms form part of the contract itself, and are
opaque to the machinery of the base layer, side-chain and associated peg transactions.

Validator nodes should expect to have to stake Tari for each contract they validate. Asset issuers will determine the
nature and amount of stake required. The contract stake is variable on a contract-to-contract basis so that an
efficient market
between asset issuers and validator nodes can develop. This market is not defined on the Tari blockchain at all and
would be implemented as a DAO on the DAN itself.

Similarly, it has been suggested in the past that Validator Nodes should post hardware benchmarks when registering. The problem
with this requirement is that it is fairly trivial to game. We cannot enforce that the machine that posted the benchmark
is the same as the one that is running validations.

A better approach is to leave this to the market. A reputation contract can be built, on Tari, of course, that
periodically and randomly asks Validator Nodes to perform cryptographically signed benchmarks in exchange for performance
certificates. Nodes can voluntarily sign up for such a service and acts as a form of credential. Nodes that do not sign
up may have trouble finding contracts to validate and might have to lower their price to get work.

Tari [contract]s are template-based, and so many contracts may wish to include templates that add one or more of the
following functionality to the side-chain contract:

* A Validator node [proof-of-participation certificate] template. Poorly performing validator nodes may receive reduced compensation,
  be fined, or even ejected from the committee at a checkpoint.
* An asset financing template. The asset issuer could provide a guaranteed pool of funds from which the committee will
  be paid at every checkpoint.
* Tari account template. This template would provide the ability for users to deposit Tari into a side-chain, withdraw
  funds at checkpoints and track their balances throughout the lifetime of the contract.

This list is far from complete, but should convey the idea that:

* Tari contracts SHOULD be highly modular and composable, with each template performing exactly ONE highly specific
  task, and doing it very well.
* The base layer and peg transactions know the absolute minimum about the assets on the chain. However, they provide
  all the information necessary for the contract templates and side-chains to function efficiently.


### The contract lifecycle
[contract lifecycle]: #the-contract-lifecycle "The contract lifecycle"

Every contract MUST be governed by one, and only one, Tari [side-chain]. A contract MAY define one or more digital assets.
This contract can be very simple or highly complex.

The lifecycle of a contract proceeds via these steps:

1. The asset issuer publishes a [contract definition transaction].
2. The asset issuer publishes a [contract constitution] transaction.
3. Once this transaction is published, we enter the [acceptance period].
4. Each validator node that will be managing the contract publishes a [contract acceptance transaction]. The group of
   validator nodes is called the Validator Node Committee (VNC).
5. Once the [acceptance period] has expired, the [peg-in period] begins.
6. The VNC jointly publishes a [peg-in transaction].
7. At this point, the contract is considered live, and users can interact with the contract on the side-chain.
8. The VNC periodically publishes a [checkpoint transaction]. 
9. Failure to do so can lead to the contract being [abandoned].
10. The VNC may opt to shut the contract down by publishing a [peg-out transaction].

The following sections will discuss each of these steps in more detail.

#### Contract instantiation
[contract instantiation]: #contract-instantiation "Contract Instantiation"

Steps 1 - 6 in the [contract lifecycle] are part of the [contract instantiation] process. It is a multi-step process 
and is ideally represented as a finite-state machine that reacts to transactions published on chain that contain outputs 
containing specific output features. The combination of output features and FSM allows nodes to accurately track the 
progress of potentially thousands of contracts in a safe and decentralised manner.


#### The contract definition transaction
[contract definition transaction]: #the-contract-definition-transaction "The contract definition transaction"

It bears repeating that every contract is governed by one, and only one, Tari [side-chain]. A contract MAY 
define one or more digital assets. These assets' behaviour is captured in [templates] and are highly composable.
This allows the contract to be very simple or highly complex, and be handled with the same contract handling machinery.

<note :tip>
The contract definition transaction defines the "what" of the digital asset set that will be created.
</note>

The contract definition transaction MUST provide 
* the full contract specification, OR a hash of the full contract specification
* and initial data. This is immutable for the lifetime of the contract.

This data tells validator nodes _exactly_ what code will be running, and the data needed to initialise that code.

Asset templates will have a strictly defined interface that includes a constructor, or initialisation method. The 
parameters that these constructors define is what determined the initial data.

These two pieces of data are _necessary_ AND _sufficient_ to enable _any_ validator node to start running the contract
and execute instructions on it.

* Every contract MUST be registered on the base layer.
* Contracts MUST be registered by publishing a `contract definition` transaction.
* The following information must be captured as part of the `contract definition` transaction in a contract
  definition UTXO:
  * Exactly ONE output MUST have a `ContractDefintion` output feature.
  * A `ContractDefintion` UTXO has the following information:
    * the asset issuer's public key, also known as the owner public key, `<PublicKey>`.
    * The contract id -- `<u256 hash>`. This is immutable for the life of the contract and is calculated as
      `H(contract_name || contract specification hash || Initial data hash)`.
    * A contract name -- `utf-8 char[32]`(UTF-8 string) 32 bytes. This is for informational purposes only, so it shouldn't
      be too long, but not too short that it's not useful (this isn't DOS 3.1 after all). 32 bytes is the same length as
      a public key or hash, so feels like a reasonable compromise.

* The [!]:
  * Version number (contract code definition can be upgraded)
  * The template hash being implemented

* The [contract code definition] also includes the initial state (hash?) for the contract.
  * e.g. all the sub-templates and their state.

The contract definition MUST UTXO hold at least the `MINIMUM_OWNER_COLLATERAL` in Tari.

The owner collateral is a small staked amount of at least `MINIMUM_OWNER_COLLATERAL`. The amount is hard-coded into
consensus rules and is a nominal amount to prevent spam, and encourages asset owners to tidy up after themselves when
a contract winds down.

Initially, `MINIMUM_OWNER_COLLATERAL` is set at 200 Tari, but MAY be changed across network upgrades.

Assuming the collateral is represented by the UTXO commitment $C = kG + vH$, the minimum requirement is verified by
having the range-proof commit to $(k, v - v_\mathrm{min})$ rather than the usual  $(k, v)$. Note that this change requires us to modify the
`TransactionOutput` definition to include a `minimum_value_commitment` field, defaulting to zero, to capture this extra information.

* The owner collateral UTXO MUST have the `TRANSACTION_DEFINITION` output feature flag set.
* The owner collateral MUST include a covenant that only permits it to be spent to a new `TRANSACTION_DEFINITION` UTXO (when
  transferring ownership of a contract), or as an unencumbered UTXO in a `CONTRACT_DEREGISTRATION` transaction. 
  TODO: This transaction is desirable because it tidies up the UTXO set. But this tx cannot be published before the VN
  shuts the contract down via a [peg-out transaction].

#### The validator committee proposal
[contract constitution]: #the-validator-committee-proposal "The validator committee proposal"

  * The asset issuer broadcasts a [contract constitution] transaction.
    * This transaction defines the "how" and "who" of the digital asset's management.
    * It links to the contract definition UTXO.
    * This transaction contains the "contract terms" for the management of the contract.
    * This transaction contains the public keys of the proposed VN committee; 
    * a expiry date before which all the VNs must sign and agree to these terms (the [acceptance period]); 
    * quorum conditions for acceptance of this proposal (default to 100%);
    * If the conditions will unequivocally pass, the waiting period MAY be shortcut.
    * this MUST be achieved by the asset issuer providing a UTXO that can only be spent by a multisig of the quorum of
      VNS performing a peg-in. There MAY be an [initial reward] that is paid to the VN committee when the UTXO is spent.
    * part of this agreement MAY be a [side-chain deposit] amount that needs to be committed as part of the [peg-in];
    * quorum conditions for peg-outs (e.g. I require a 3 of 5 threshold signature); In this instance peg-outs refer to an exit
      of the entire side-chain. We envisage that depositing and withdrawing funds into / out of the SC can be done via
      a template and will vary depending on the template (e.g. custodial vs self-custody / refund tx).
    * The advertised consensus model for the side chain (this can't be enforced, but can be checked); 
      * including checkpoint quorum requirements
      * Checkpoint parameters, including, frequency, rules around committee changes.

If both the [acceptance period] and [peg-in period] elapses and quorum, the asset owner MAY spend the validator 
committee proposal UTXO back to himself to recover his funds.

#### The contract acceptance transaction
[contract acceptance transaction]: #the-contract-acceptance-transaction "The contract acceptance transaction"

The asset issuer must then find a set of validator nodes that will manage the contract. This is typically done 
out-of-band, via a DEX, DAO or other marketplace. The VNs could also be owned by the issuer itself.

  * Validator nodes MUST cryptographically [acknowledge and agree] to manage the contract.
  * Each VN publishes a [contract acceptance transaction] committing the required stake. The UTXO is also an explicit 
      agreement to manage the contract.
  * In a PoW side chain, we still need a VN to act as "checkpoint publisher", so this step is still required for PoW 
      chains.
  * The UTXO has a time lock, that prevents the VN spending the UTXO before the [acceptance period] + [peg-in period] 
      has lapsed.

#### The peg-in period
[peg-in period]: #the-peg-in-period "The peg-in period"

Once the [acceptance period] has expired, [peg-in period] begins.

At this point, VNs that have accepted the contract must
    * allocate resources
    * Setup whatever is needed to run the contract
    * Set up consensus with their peer VNs (e.g. hotstuff)
    * Intialise the contract and run the constructors
    * Reach consensus on the initial state.
    * Prepare the [peg-in] transaction.

all before the [peg-in] period expires.
  
#### The peg-in transaction
[peg-in transaction]: #the-peg-in-transaction "The peg-in transaction"
  
Side-chains MUST be initiated by virtue of a [peg-in] transaction.

* Once the [acceptance period] has expired, [peg-in period] begins.
* At this point, there MUST be a quorum of acceptance transactions from validator nodes. 
  validator node committee MUST collaborate to produce, sign and broadcast the peg-in transaction by spending the 
  [initial reward]. This also serves the purpose of linking the peg-in to the specific contract that is being 
  managed by the side-chain.
* There is a minimum [side-chain deposit] that MUST be included in the peg-in UTXO. A single aggregated UTXO 
  containing at least $$ m D $$ Tari, where _m_ is the number of VNs and _D_ is the deposit required.
* This transaction also acts as the zero-th checkpoint for the contract. As such, it requires all the checkpoint 
  information.
* The state commitment is the merklish root of the state after running the code initialisation using the [initial 
  data] provided in the [contract definition].

#### Checkpoint transactions
[checkpoint]: #checkpoint-transactions "Checkpoint transactions"

The roles of the checkpoint transaction:
* Present proof of liveness
* Allows VNCs to make changes to the committee
* Summarise contract state
* Optionally summarise contract logs / events

Note: In the discussion of Tari account contract templates below, we need a mechanism for proving that side-chain state
corresponds to what someone is claiming wrt a valid L1 transaction. But, since our policy is one that the base layer
never knows anything about what goes on in SCs (or that SCs even exist), this poses a challenge. One elegant solution
to this would be to add a `MERKLE_PROOF` opcode to TariScript that could validate an L1 transaction based on a 
checkpoint merkle root combined with a merkle proof that a VNC has given to a user.


* Validator node committees MUST periodically sign and broadcast a [checkpoint] transaction.
  * The transaction signature MUST satisfy the requirements laid out for checkpoint transactions defined in the 
    [contract constitution].  (alt names: Contract Governance Manifesto / Contract Management Manifesto)
  * The checkpoint UTXO MUST have the `CHECKPOINT` output feature.
  * It MUST reference the [contract_id].
  * This feature contains a commitment to the current contract state. This is typically some sort of Merklish root.
  * It MAY have a URI to off-chain state or merkle tree
  * A checkpoint number, strictly increasing by 1 from the previous checkpoint.
  * The checkpoint MUST spend the previous checkpoint. This is guaranteed by virtue of a covenant. The contract id 
    must equal the contract id of the checkpoint being spent.
  * The checkpoint UTXO contains a covenant that provides the spending conditions described above.
  * Checkpoints allow the exit and entrance of new validator nodes into the committee.
  * Checkpoints MAY also contain [contract constitution] change pipelines.
* The validator node committee MUST post periodic [checkpoints] onto the base layer.
  * The checkpoint MUST include a [summary of the contract state]. This summary SHOULD be in the form of a Merklish Root.
* If a valid checkpoint is not posted within the maximum allowed timeframe, the contract is [abandoned]. This COULD lead
  to penalties and stake slashing if enabled within the contract specification.

##### VNC management
Removal or addition of VNC members MUST happen at checkpoints. (In general, the same applies to any changes to the 
[contract constitution]).

* The rules over how members are added or removed are defined in the [contract constitution]. It may be that
  the VNC has autonomy over these changes, or that the asset issuer must approve changes, or some other authorization 
  mechanism.
* At the minimum, there's a proposal step, a validation step, an acceptance step, and an activation step. Therefore
  changes take place over at least a 4-checkpoint time span.
* If a VN leaves a committee their [side-chain deposit] MAY be refunded to them.
* If a new VN joins the committee they must provide the [side-chain deposit] at their activation step.
* In the proposal step, any authorised VNC change proposer, as defined in the [contract constitution]

##### Contract abandonment
If a contract misses one or more checkpoints, nodes can mark it as `abandoned`. This is not formally marked on the 
blockchain, (since something was NOT done on-chain), but nodes will be able to test for abandoned state.

The [contract constitution] COULD provide a set of emergency pubkeys that are able to 
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


### Other considerations and specifications

The following requirements aren't part of the contract lifecycle specifically, but are needed to remove ambiguity.

#### Template code registration and versioning

The code template implementations MUST be registered on the base layer.

The reason for this is that it allows Validator Nodes to know unequivocally that they are all running the same code
and can expect the same output for the same input.

Template registration also allows us to implement a secure and trust-minimised upgrade mechanism for templates. 

Potentially, we could even introduce a mechanism wherein template developers get paid for people using their template.

Template registration UTXO would contain:
  * A link to the code (git commit or IPFS)
  * The type of code (source or binary blob)
  * A hash of the source code / blob
  * Version info.
  * [Execution engine] requirements (similar to solc pragma)

There's a clear upgrade path, since
there's a code-chain from one version of a contract template to the next.

#### Contract user accounts
[contract user accounts]: #contract-user-accounts

Tari uses the UTXO model in its ledger accounting. On the other hand Tari side-chains SHOULD use an account-based system
to track balances and state.

The reasons for this are:

* An account-based approach leads to fewer outputs on peg-out transactions. There is roughly a 1:1 ratio of users
  to balances in an account-based system. On the other hand there are O(n) UTXOs in an output-based system where `n` are
  the number of transactions carried out on the side-chain. When a side-chain wants to shut down, they must record a new
  output on the base layer for every account or output (as the case may be) that they track in the peg-out transaction(s).
  It should be self-evident that account-based systems are far more scalable in the vast majority of use-cases.
* Following on from this, Accounts scale better for micro-payment applications, where hundreds or thousands of tiny 
  payments flow between the same two parties.
* Many DAN applications will want to track state (such as NFTs) as well as currency balances. Account-based ledgers make
  this type of application far simpler.

##### Pedersen commitments and account-based ledgers

Standard Pedersen commitments are essentially useless in account-based ledgers.

The reason being that since the spending keys would be common to all transactions involving a given account, it is trivial
to use the accounting rules to cancel out the `k.G` terms from transactions and to use a pre-image attack to unblind all
the values.

The specific protocol of user accounts in the side-chain is decided by the asset issuer.

Options include:

* Fully trusted

In this configuration, the side-chain is controlled by a single validator node, perhaps a server running an RDMS.
The validator node has full visibility into the state of the side chain at all times. It may or may not share this
state with the public. If it does not, then the situation is analogous to current Web 2.0 server applications.

* Decentralised and federated

In this configuration, a distributed set of validator nodes maintain the side-chain state. The set of nodes are fixed.
If consensus between nodes is achieved using a mechanism such as HotStuff BFT, very high throughputs can be achieved.

* Decentralised and censorship resistant

In this configuration, the side-chain could itself be a proof-of-work blockchain. This offers maximum decentralisation,
and censorship resistance. However, throughput will be lower.

* Confidentiality

As mentioned above, Pedersen commitments are not suitable for account-based ledgers. However, the [Zether] protocol
was expressly designed to provide confidentiality in a smart-contract context. It can be combined with any of the above
schemes. Zether can also be [extended](https://github.com/ConsenSys/anonymous-zether) to provide privacy by including
a ring-signature scheme for transfers.

### Funding, withdrawals and deposits

Deposits and withdrawals go via a smart contract template.

#### Very high level flow

1. Send Tari via OSP to VNC address. (Could have a `DEPOSIT` output feature if required)
2. VNC sees this, and then issues / prints / mints equivalent value on side-chain according to the SC protocol.
3. Equivalent coins change hands many times. VNC keeps track
4. User requests a withdrawal. 
5. VNC "burn" equivalent coins on SC.
6. Broadcasts standard OSP Tari tx to user.

7. Optionally, provide proofs of reserve / locked funds === printed funds.
This trusts the VNC completely. Akin to deposited funds on Coinbase.

Variants
* Users deposit and get a refund transaction to hold onto.
* The refund tx gets updated every time the balance changes. ala Lightning.
* Proof of burn tied to proof of spend.
* Atomic swaps to force issue of token on side-chain in (1.) above.

We could implement any/all of these variants in different templates.

TODO - fees
2 Template models:
 - Model A - Centrally funded
 - Fees are drawn from a single account (typically funded by asset issuer)
 - Eligible instructions are defined in the template constructor.

 - Model B - User funded
 - Requires an account template
 - Fees are supplied with an instruction
 - Eligible instructions are defined in the template constructor.

 - Instructions that are not covered by the model MAY be rejected by the VNC

What does an instruction look like?

Note: Solana instructions contain
 - ProgramId
 - Vec of accounts that the instruction will interact with (plus whether they're mutable and have signer auth)
 - a blob that the program will deserialise.
So, no inherently accessible API

Requires:
 - Contract ID
 - Vec of method calls: (this is different to how Solana does it/ Maybe some discussion on pros&cons is worthwhile. 
   If we go WASM, the API is available via reflection)
   - Method ID (template::method)
   - Method arguments
 - Authorization
   - signed token-based (Macaroons / JWTish)

Now the VNs have everything they need to execute the Instruction. 
They execute the instruction.
The update the state tree.
Return of the call is a "diff" of some sort, which gets appended to the "OP Log" document, 
and the new state root hash.

The VNCs MUST reach consensus on this result.

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
  
  
#### Cross-side-chain interactions

Possible routes for this:

##### Atomic transactions 
  - Provide a proof that a conditional instruction on one chain has been executed,
  - Execute on this chain, which reveals some fact that the other chain can use to finalise the instruction on the
    other chain.
  - Rolls back if 2nd party does not follow through.

Advantages:
- Does work. 

Disadvantages
- Slow
- Need to get data from other chain.
- Might hold up entire chain for extended periods.

##### Observer protocol
Implement a set of APIs in a template for reading the event log from the VNC directly or query the "read-only" contract.

Pros:
- Fast
- Permissionless in one-way applications
- Can check that results are signed by the VNC quorum

Cons:
- Rely on contracts implementing the protocol
- Instructions that require both chains' state to update is harder using this method.

#### Micro-payment

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







#### Validator Node collateral

[RFC-0001]: RFC-0001_overview.md
[the role of the base layer]: RFC-0001_overview.md#the-role-of-the-base-layer
[Zether]: https://eprint.iacr.org/2019/191.pdf
[comms network]: RFC-0172_PeerToPeerMessagingProtocol.md
