# Tari Network Terminology

Below are a list of terms and their definitions that are used throughout the Tari code and documentation. Let's use this
glossary to disambiguate ideas, and work towards a
[ubiquitous language](https://blog.carbonfive.com/2016/10/04/ubiquitous-language-the-joy-of-naming/) for this project.

## Asset Issuer
[Asset Issuer]: #asset-issuer 'An entity that creates digital assets on the Tari DAN'

An entity that creates digital assets on the Tari DAN. The Asset Issuer will specify the parameters of the contract template
that defines the rules that govern the asset and the number and nature of its constituent tokens on issuance. The Asset Issuer
will, generally, be the initial owner of the tokens.

## Bad Actor
[Bad Actor]: #bad-actor 'A stakeholder that acts maliciously or negligently to the detriment of the network or another stakeholder'

A stakeholder that acts maliciously or negligently to the detriment of the network or another stakeholder.

## Base layer
[Base Layer]: #base-layer 'The Tari layer handling payments and secured by proof of work'


The Tari Base layer is a merge-mined [blockchain] secured by proof-of-work. The base layer is primarily responsible for
the emission of new Tari, for securing and managing [Tari coin] transfers.


## Base Node
[base node]: #base-node 'A full Tari node running on the base layer, validating and propagating Tari coin transactions and blocks'

A full Tari node running on the base layer. It's primary role is validating and propagating [Tari coin] transactions
and blocks to the rest of the network.


## Block
[block]: #block 'A collection transactions and associated metadata recorded as a single entity in the Tari blockchain'

A collection transactions and associated metadata recorded as a single entity in the Tari blockchain. The ordering of
 Tari transactions is set purely by the block height of the block they are recorded in. 


## Block reward
[block reward]: #block-reward 'The amount of Tari created in every block'

The amount of Tari created by the coinbase transaction in every block. The block reward is set by the 
[emission schedule].


## Blockchain
[blockchain]: #blockchain 'The linked sequence of Tari blocks on the Tari base layer'

A sequence of tari [block]s. Each block contains a hash of the previous valid block. Thus the blocks form a chain 
with the property that changing anything in a block other than the head block requires rewriting the entire 
blockchain from that point on.

## Checkpoint
[checkpoint]: #checkpoint 'A summary of the state of a Digital Asset that is recorded on the base layer'

A hash of the state of a Digital Asset that is recorded on the base layer.

## Coinbase transaction 
[coinbase transaction]: #coinbase-transaction

The first transaction in every Tari block yields a [Block Reward] according to the Tari [emission Schedule] and is 
awarded to the miner that performed the Proof of Work for the block.

## Committee
[committee]: #committee 'A group of validator nodes that are responsible for managing a specific Digital Asset'

A group of [Validator Node]s that are responsible for managing the state of a specific [Digital Asset]. A committee is selected
during asset issuance and can be updated at [Checkpoint]s.

## CommitteeSelectionStrategy
[CommitteeSelectionStrategy]: #committeeselectionstrategy 'A strategy for the DAN to algorithmically select candidates for the committee from the available registered Validator Nodes'
A strategy for the DAN to algorithmically select candidates for the committee from the available registered Validator Nodes. The VNs will need accept the nomination to become part of the committee.

## ConsensusStrategy
[ConsensusStrategy]: #consensusstrategy 'The approach that will be taken for a committee to reach consensus on instructions'

The approach that will be taken for a committee to reach consensus on the validity of instructions that are performed on a
given Digital Asset.

## Digital asset
[digital asset]: #digital-asset 'Sets of Native digital tokens, both fungible and non-fungible that are created by 
asset issuers on the Tari 2nd layer'

Digital assets (DAs) are the sets or collections of native digital tokens (both fungible and non-fungible) that are 
created by [asset issuer]s on the Tari 2nd layer. For example, a promoter might create a DA for a music concert event. The
 event is the digital asset, and the tickets for the event are digital asset [tokens].


## Digital Asset Network
[Digital Asset Network]: #digital-asset-network 'The Tari second layer. All digital asset interactions are managed here.'

The Tari second layer. All digital asset interactions are managed on the Tari Digital Assets Network (DAN). These
interactions (defined in [instruction]s) are processed and validated by [Validator Node]s.

## DigitalAssetTemplate
[DigitalAssetTemplate]: #digitalassettemplate 'A set of non-turing complete contract types supported by the DAN'

A DigitalAssetTemplate is one of a set of contract types supported by the DAN. These contracts are non-turing complete and consist of
rigid rule-sets with parameters that can be set by Asset Issuers.

## Digital asset tokens
[tokens]: #digital-asset-tokens 'or just, "tokens". The tokens associated with a given digital asset. Tokens are created
 by asset issuers'

Digital asset tokens (or often, just "tokens") are the finite set of digital entities associated with a given digital 
asset. Depending on the DA created, tokens can represent tickets, in-game items, collectibles or loyalty points. They
 are bound to the [digital asset] that created them.


## Instructions
[instruction]: #instructions 'Second-layer network commands for managing digital asset state'

Instructions are the [digital asset network] equivalent of [transaction]s. Instructions are issued by asset issuers and
client applications and are relayed by the DAN to the [validator node]s that are managing the associated
[digital asset].


## Emission schedule
[emission schedule]: #emission-schedule 

An explicit formula as a function of the block height, _h_, that determines the block reward for the 
_h_<sup>th</sup> block.


## MimbleWimble
[mimblewimble]: #mimblewimble 'a privacy-centric cryptocurrency protocol'

MimbleWimble is a privacy-centric cryptocurrency protocol. It was
[dropped](https://download.wpsoftware.net/bitcoin/wizardry/mimblewimble.txt) in the Bitcoin Developers chatroom by an
anonymous author and has since been refined by several authors, including Andrew Poelstra.

## RegistrationCollateral
[RegistrationCollateral]: #registrationcollateral 'An amount of tari coin that is locked up on the base layer when a [Validator Node] is registered'

In order to make Sybil attacks expensive and to provide an authorative base layer registry of [validator node]s they will need to lock up
a amount of [Tari Coin] on the [Base Layer] using a registration transaction to begin acting as a VN on the DAN.

## RegistrationTerm
[RegistrationTerm]: #registrationterm 'The minimum amount of time that a VN registration lasts'

The minimum amount of time that a VN registration lasts, the RegistrationCollateral can only be released after this minimum period has elapsed.

## Transaction
[transaction]: #transaction 'Base layer tari coin transfers.'

Transactions are activities recorded on the Tari [blockchain] running on the [base layer]. Transactions always involve a
transfer of [Tari coin]s.


## Tari Coin
[tari coin]: #tari-coin 'The base layer token'

The base layer token. Tari coins are released according to the [emission schedule] on the Tari [base layer] 
[blockchain] in [coinbase transaction]s.

## Trusted Node
[trusted node]: #trusted-node 'A permissioned Validator Node nominated by an Asset Issuer'

A permissioned Validator Node nominated by an Asset Issuer that will form part of the committee for that Digital Asset.

## Unspent transaction outputs
[utxo]: #unspent-transaction-outputs

An unspent transaction output (UTXO) is a discrete number of Tari that are available to be spent. The sum of all 
UTXOs represents all the Tari currently in circulation. In addition, the sum of all UTXO values equals the sum of the
 [block reward]s for all blocks up to the current block height.
 
UTXO values are hidden by their commitments. Only the owner of the UTXO and (presumably) the creator of the UTXO 
(either a [Coinbase transaction] or previous spender) know the value of the UTXO.


## Validator Node
[validator node]: #validator-node 'A second-layer node that manages and validates digital asset state transitions'

Validator nodes (VNs) make up the Tari second layer, or [Digital Asset Network]. VNs are responsible for creating and
updating [digital asset]s living on the Tari network.


# Disclaimer

This document is subject to the [disclaimer](DISCLAIMER.md).