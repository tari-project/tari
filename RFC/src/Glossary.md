# Tari Network Terminology

Below are a list of terms and their definitions that are used throughout the Tari code and documentation. Let's use this
glossary to disambiguate ideas, and work towards a
[ubiquitous language](https://blog.carbonfive.com/2016/10/04/ubiquitous-language-the-joy-of-naming/) for this project.

## Archive node

[archivenode]: #archive-node "a full history node"

This is a full history [base node]. It will keep a complete history of every transaction ever received and it will not
implement pruning.

## AssetCollateral

[assetcollateral]: #assetcollateral

The amount of tari coin that a [Validator Node] must put up on the [base layer] in order to become part of an asset
[committee].

## Asset Issuer

[asset issuer]: #asset-issuer "An entity that creates digital assets on the Tari DAN"

An entity that creates digital assets on the Tari DAN. The Asset Issuer will specify the parameters of the contract
template that defines the rules that govern the asset and the number and nature of its constituent tokens on issuance.
The Asset Issuer will, generally, be the initial owner of the tokens.

## Bad Actor

[bad actor]: #bad-actor "A participant that acts maliciously or negligently to the detriment of the network or another
participant"

A participant that acts maliciously or negligently to the detriment of the network or another participant.

## Base layer

[base layer]: #base-layer "The Tari layer handling payments and secured by proof of work"

The Tari Base layer is a merge-mined [blockchain] secured by proof-of-work. The base layer is primarily responsible for
the emission of new Tari, for securing and managing [Tari coin] transfers.

## Base Node

[base node]: #base-node "A full Tari node running on the base layer, validating and propagating Tari coin transactions
and blocks"

A full Tari node running on the base layer. It's primary role is validating and propagating [Tari coin] transactions
and blocks to the rest of the network.

## Block

[block]: #block "A collection of transactions and associated metadata recorded as a single entity in the Tari blockchain"

A collection of transactions and associated metadata recorded as a single entity in the Tari blockchain. The ordering of
Tari transactions is set purely by the block height of the block they are recorded in.

## Block Header

[block header]: #block-header "A collection of information validating the information within a [block]"

A data structure that validates the information contained in a [block].

## Block Body

[block body]: #block-body "The transaction inputs, outputs, and kernels that make up the block"

A data structure containing the transaction inputs, outputs, and kernels that make up the block.

## Block reward

[block reward]: #block-reward "The amount of Tari created in every block"

The amount of Tari created by the coinbase transaction in every block. The block reward is set by the
[emission schedule].

## Blockchain

[blockchain]: #blockchain "The linked sequence of Tari blocks on the Tari base layer"

A sequence of tari [block]s. Each block contains a hash of the previous valid block. Thus the blocks form a chain
with the property that changing anything in a block other than the head block requires rewriting the entire
blockchain from that point on.

## Blockchain state

[blockchainstate]: #blockchain-state "This is a snapshot of how the blockchain looks"

The complete state of the blockchain at a specific block height. This means a pruned [utxo] set, a complete set of
kernels and headers up to that block height from the genesis block.

## BroadcastStrategy

[broadcast strategy]: #broadcaststrategy "Strategy used for broadcasting messages in a peer-to-peer network"

A strategy for propagating messages amongst nodes in a peer-to-peer network. Example implementations of
`BroadcastStrategy` include the Gossip protocol and flood fill.

## Chain Reorganization
[reorg]: #chain-reorg "After a chain split occurs nodes on the poorer chain must resync to the best chain"

A chain reorganization occurs after a chain split occurs on the network, which commonly occurs due to network latency
and connectivity issues. When a chain split occurs one chain will have the higher accumulated proof-of-work, this chain
is considered the best chain. Nodes on the poorer chain will need to rewind and resync their chains to best chain. In 
this process transaction in the mempool could become orphaned or invalid.

## Checkpoint

[checkpoint]: #checkpoint "A summary of the state of a Digital Asset that is recorded on the base layer"

A hash of the state of a Digital Asset that is recorded on the base layer.

## Coinbase transaction

[coinbase transaction]: #coinbase-transaction

The first transaction in every Tari block yields a [Block Reward] according to the Tari [emission Schedule] and is
awarded to the miner that performed the Proof of Work for the block.

## Committee

[committee]: #committee "A group of validator nodes that are responsible for managing a specific Digital Asset"

A group of [Validator Node]s that are responsible for managing the state of a specific [Digital Asset]. A committee is
selected during asset issuance and can be updated at [Checkpoint]s.

## CommitteeSelectionStrategy

[committeeselectionstrategy]: #committeeselectionstrategy "A strategy for an Asset Issuer to select candidates for the
committee from the available registered Validator Nodes who responded to the nomination call for that asset"

A strategy for an Asset Issuer to select candidates for the committee from the available registered Validator Nodes who
responded to the nomination call for that asset.

## ConsensusStrategy

[consensusstrategy]: #consensusstrategy "The approach that will be taken for a committee to reach consensus on
instructions"

The approach that will be taken for a committee to reach consensus on the validity of instructions that are performed
on a given Digital Asset.

## Commitment

[commitment]: #commitment

A commitment is a cryptographic primitive that allows one to commit to a chosen value while keeping it hidden from
others, with the ability to reveal the committed value later. Commitments are designed so that one cannot change the
value or statement after they have committed to it.

## Communication Node

[communication node]: #communication-node "A communication node that is responsible for maintaining the Tari
communication network"

A Communication Node is either a Validator Node or Base Node that is part of the Tari communication network. It
maintains the network and is responsible for forwarding and propagating joining requests, discovery requests and data
messages on the communication network.

## Communication Client

[communication client]: #communication-client "A communication client that makes use of the Tari communication network,
but does not maintain it"

A Communication Client is a Wallet or Asset Manager that makes use of the Tari communication network to send joining and
discovery requests. A Communication Client does not maintain the communication network and is not responsible for
forwarding or propagating any requests or data messages.

## Creator Nomination Mode

[creator nomination mode]: #creator-nomination-mode "An asset runs in creator nomination mode when _every_ validator
node in a validator committee is a [Trusted Node] that was directly nominated by the AI."

An asset runs in creator nomination mode when _every_ validator node in a validator committee is a [Trusted Node] that
was directly nominated by the [Asset Issuer].

## Current head

[currenthead]: #current-head "The last valid block of the longest chain"

The last [block] of the base layer that represents the latest valid block. This [block] must be from the longest
proof-of-work chain to be the current head.

## Cut-Through

[cut-through]: #cut-through "Cut-through is the process where outputs may be omitted"

Cut-through is the process where outputs spent within a single block may be removed without breaking the standard [MimbleWimble](#mimblewimble)
validation rules. Simplistically, `Alice -> Bob -> Carol` may be "cut-through" to `Alice -> Carol`. Bob's commitments may be removed.

On Tari, for reasons described in [RFC-0201_TariScript](./RFC-0201_TariScript.md#utxo-data-commitments), cut-through is prevented from ever happening.

## Digital asset

[digital asset]: #digital-asset "Sets of Native digital tokens, both fungible and non-fungible that are created by
asset issuers on the Tari 2nd layer"

Digital assets (DAs) are the sets or collections of native digital tokens (both fungible and non-fungible) that are
created by [asset issuer]s on the Tari 2nd layer. For example, a promoter might create a DA for a music concert event.
The event is the digital asset, and the tickets for the event are digital asset [tokens].

## Digital Asset Network

[digital asset network]: #digital-asset-network "The Tari second layer. All digital asset interactions are managed here."

The Tari second layer. All digital asset interactions are managed on the Tari Digital Assets Network (DAN). These
interactions (defined in [instruction]s) are processed and validated by [Validator Node]s.

## DigitalAssetTemplate

[digitalassettemplate]: #digitalassettemplate "A set of non-turing complete contract types supported by the DAN"

A DigitalAssetTemplate is one of a set of contract types supported by the DAN. These contracts are non-turing complete
and consist of rigid rule-sets with parameters that can be set by Asset Issuers.

## Digital asset tokens

[tokens]: #digital-asset-tokens 'or just, "tokens". The tokens associated with a given digital asset. Tokens are created
 by asset issuers'

Digital asset tokens (or often, just "tokens") are the finite set of digital entities associated with a given digital
asset. Depending on the DA created, tokens can represent tickets, in-game items, collectibles or loyalty points. They
are bound to the [digital asset] that created them.

## Hashed Time Locked Contract

[htlc]: #hashed-time-locked-contract 'or just, "HTLC".'

A time locked contract that only pays out after a certain criteria has been met or refunds the originator if a certain
period has expired.

## Emission schedule

[emission schedule]: #emission-schedule

An explicit formula as a function of the block height, _h_, that determines the block reward for the
_h_<sup>th</sup> block.

## Instructions

[instruction]: #instructions "Second-layer network commands for managing digital asset state"

Instructions are the [digital asset network] equivalent of [transaction]s. Instructions are issued by asset issuers and
client applications and are relayed by the DAN to the [validator node]s that are managing the associated
[digital asset].

## Mempool

[mempool]: #mempool "A memory pool for unconfirmed transactions on the base layer"

The mempool consists of the transaction pool, pending pool, orphan pool and reorg pool, and is responsible for managing
unconfirmed transactions that have not yet been included in the longest proof-of-work chain. Miners usually draw
verified transactions from the mempool to build up transaction [block]s.

## Metadata Signature

[metadata signature]: #metadata-signature

The metadata signature is an aggregated Commitment Signature ("ComSig") signature, attached to a transaction output and
signed with a combination of the homomorphic commitment private values \\( (v\_i \\, , \\, k\_i )\\), the spending key
known only to the receiver, and sender offset private key \\(k\_{Oi}\\) known only to the sender. This prevents
malleability of the UTXO metadata.

## Mimblewimble

[mimblewimble]: #mimblewimble "a privacy-centric cryptocurrency protocol"

Mimblewimble is a privacy-centric cryptocurrency protocol. It was
[dropped](https://download.wpsoftware.net/bitcoin/wizardry/mimblewimble.txt) in the Bitcoin Developers chatroom by an
anonymous author and has since been refined by several authors, including Andrew Poelstra.

## Mining Server

[mining server]: #mining-server

A Mining Server is responsible for constructing new blocks by bundling transactions from the [mempool] of a connected
[Base Node]. It also distributes Proof-of-Work tasks to Mining Workers and verifies PoW solutions.

## Mining Worker

[mining worker]: #mining-worker

A Mining Worker is responsible for performing Proof-of-Work tasks received from its parent [Mining Server].

## Multisig

[multisig]: #multisig

Multi-signatures (Multisigs) are also known as N-of-M signatures, this means that a minimum of N number of the M peers
need to agree before a transaction can be spent. N and M can be equal; which is a special case and is often referred to
as an N-of-N Multisig.

[TLU musig](https://tlu.tarilabs.com/cryptography/musig-schnorr-sig-scheme/The_MuSig_Schnorr_Signature_Scheme.html)

## Node ID

[node id]: #node-id

A node ID is a unique identifier that specifies the location of a [communication node] or [communication client] in the
Tari communication network. The node ID can either be obtained from registration on the [Base Layer] or can be derived
from the public identification key of a [communication node] or [communication client].

## Non-fungible Token (NFT)

[non fungible token]: #nft

A Non-fungible token is a specific instance of a token issued as part of a [digital asset]. It is another name for a
[digital asset token]. NFTs are contained within specially marked [UTXO]s on the Tari Base Layer.

## Orphan Pool

[orphan pool]: #orphan-pool "A pool in the Mempool for unconfirmed transactions that attempt to spend non-existent UTXOs"

The orphan pool is part of the [mempool] and manages all [transaction]s that have been verified but attempt to spend
[UTXO]s that do not exist or haven't been created yet.

## Pending Pool

[pending pool]: #pending-pool "A pool in the Mempool for unconfirmed transactions with time-lock restrictions"

The pending pool is part of the [mempool] and manages all [transaction]s that have a time-lock restriction on when it
can be processed or attempts to spend [UTXO]s with time-locks.


[pruninghorizon]: #pruning-horizon "Block height at which pruning will commence"

This is a local setting for each node to help reduce syncing time and bandwidth. This is the number of blocks from the
chain tip beyond which a chain will be pruned.

## Public Nomination Mode

[public nomination mode]: #public-nomination-mode

An asset runs in public nomination mode when the [Asset Issuer] broadcasts a call for nominations to the network and VNs
from the network nominate themselves as candidates to become members of the [committee] for the asset. The
[Asset Issuer] will then employ the [CommitteeSelectionStrategy] to select the committee from the list of available
candidates.

## Range proof

[range proof]: #range-proof

A mathematical demonstration that a value inside a [commitment] (i.e. it is hidden) lies within a certain range. For
[Mimblewimble], range proofs are used to prove that outputs are positive values.

## Registration Deposit

[registration deposit]: #registration-deposit "An amount of tari coin that is locked up on the base layer when a
[Validator Node] is registered"

An amount of tari coin that is locked up on the base layer when a [Validator Node] is registered. In order to make Sybil
attacks expensive and to provide an authorative base layer registry of [validator node]s they will need to lock up a
amount of [Tari Coin] on the [Base Layer] using a registration transaction to begin acting as a VN on the DAN.

## Registration Term

[registration term]: #registration-term "The minimum amount of time that a VN registration lasts"

The minimum amount of time that a VN registration lasts, the [Registration Deposit] can only be released after this
minimum period has elapsed.

## Reorg Pool

[reorg pool]: #reorg-pool "A backup pool in the Mempool for unconfirmed transactions that have been included in blocks"

The reorg pool is part of the [mempool] and stores all [transaction]s that have recently been included in blocks in case
a blockchain reorganization occurs and the transactions need to be restored to the [transaction pool].

## Script Keypair

[script key]: #script-keypair

The script private - public keypair, \\((k\_{Si}\\),\\(K\_{Si})\\), is used in [TariScript] to unlock and execute the
script associated with an output. Afterwards the execution stack must contain exactly one value that must be equal to
the script public key.

## Script Offset

[script offset]: #script-offset

The script offset provides a proof that every script public key \\( K\_{Si} \\) and sender offset public key
\\( K\_{Oi} \\) provided for the a transaction's inputs and outputs are correct.

## Sender Offset Keypair

[sender offset key]: #sender-offset-keypair

The sender offset private - public keypair, (\\( k\_{Oi} \\),\\( K\_{Oi} \\)), is used by the sender of an output to
lock all its metadata by virtue of a [sender metadata signature].

## Spending Key

[spending key]: #spending-key

A private spending key is a private key that permits spending of a [UTXO]. It is also sometimes referred to as a
Blinding Factor, since is Tari (and Mimblewimble) outputs, the value of a UTXO is _blinded_ by the spending key:

$$ C = v.H + k.G $$

The public key, \\(P = k.G\\) is known as the _public_ spending key.

## SynchronisationState

[synchronisationstate]: #synchronisationstate

The current synchronisation state of a [Base Node]. This can either be

* `starting` - The node has freshly started up and is still waiting for first round of chain_metadata responses from its 
  neighbours on which to base its next state change.
* `header_sync` - The node is in the process of synchronising headers with chosen sync peer.
* `horizon_sync` - The node is in the process of syncing blocks from the tip to its [pruning horizon]
* `block_sync` -   The node is in the process of syncing all blocks back to the genesis block
* `listening` - The node has completed its syncing strategy and will continue to listen for new blocks and monitor
  its neighbours to detect if it falls behind.


## SynchronisationStrategy

[synchronisationstrategy]: #synchronisationstrategy

The generalised approach for a [Base Node] to obtain the current state of the blockchain from the peer-to-peer network.
Specific implementations may differ based on different trade-offs and constraints with respect to bandwidth, local
network conditions etc.

## Tari Coin

[tari coin]: #tari-coin "The base layer token"

The base layer token. Tari coins are released according to the [emission schedule] on the Tari [base layer]
[blockchain] in [coinbase transaction]s.

## TariScript

[tariscript]: #tariscript "The Tari scripting system for transactions"

Tari uses a scripting system for transactions, not unlike [Bitcoin's scripting system](https://en.bitcoin.it/wiki/Script),
called TariScript. It is also simple, stack-based, processed from left to right, not Turing-complete, with no loops. It
is a list of instructions linked in a non&nbsp;malleable way to each output, specifying its conditions of spending.

## Transaction

[transaction]: #transaction "Base layer tari coin transfers."

Transactions are activities recorded on the Tari [blockchain] running on the [base layer]. Transactions always involve a
transfer of [Tari coin]s. A [mimblewimble](#mimblewimble) transaction body consists of one or more blinded inputs and outputs.

## Transaction Pool

[transaction pool]: #transaction-pool "A pool in the Mempool for valid and verified unconfirmed transactions"

The transaction pool is part of the [mempool] and manages all [transaction]s that have been verified, that spend valid
[UTXO]s and don't have any time-lock restrictions.

## Trusted Node

[trusted node]: #trusted-node "A permissioned Validator Node nominated by an Asset Issuer"

A permissioned Validator Node nominated by an Asset Issuer that will form part of the committee for that Digital Asset.

## Token Wallet

[token wallet]: #token-wallet "An Asset Manager Wallet for Tari Assets and Tokens"

A Tari Token Wallet is responsible for managing [Digital asset]s and [Tokens], and for constructing and negotiating
[instruction]s for transferring and receiving Assets and Tokens on the [Digital Asset Network].

## Transaction Weight

[transaction weight]: #transaction-weight "Transaction "

The weight of a transaction / block measured in "grams". 
See [Block / Transaction weight](./RFC-0110_BaseNodes.md#blocktransaction-weight) for more details.

## Unspent transaction outputs

[utxo]: #unspent-transaction-outputs

An unspent transaction output (UTXO) is a discrete number of Tari that are available to be spent. The sum of all
UTXOs represents all the Tari currently in circulation. In addition, the sum of all UTXO values equals the sum of the
[block reward]s for all blocks up to the current block height.

UTXO values are hidden by their [commitment]s. Only the owner of the UTXO and (presumably) the creator of the UTXO
(either a [Coinbase transaction] or previous spender) know the value of the UTXO.

## Validator Node

[validator node]: #validator-node "A second-layer node that manages and validates digital asset state transitions"

Validator nodes (VNs) make up the Tari second layer, or [Digital Asset Network]. VNs are responsible for creating and
updating [digital asset]s living on the Tari network.

## Wallet

[wallet]: #wallet "A Wallet for Tari coins"
[registration deposit]: #registration-deposit

A Tari Wallet is responsible for managing key pairs, and for constructing and negotiating [transaction]s for
transferring and receiving [tari coin]s on the [Base Layer].

# Disclaimer

This document is subject to the [disclaimer](../DISCLAIMER.md).
