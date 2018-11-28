# The Tari protocol

## Code organisation

The code follows a domain-driven design layout, with top-level folders falling into infrastructure, domain, or
application layers.

The `infrastructure` folder contains code that is not Tari-specific. It holds the following crates:
* `comms`: The networking and messaging subsystem
* `crypto`: All cryptographic services, including a Curve25519 implementation
* `storage`: Data persistence services, including LMDB

The `base_layer` is a domain-level folder and contains:
* `core`: common classes and traits, such as `Transaction`s and `Block`s
* `blockchain`: The Tari consensus code
* `mempool`: The unconfirmed transaction pool implementation
* `mining`: The merge-mining modules
* `p2p`: The block and transaction propagation module
* `api`: interfaces for clients and wallets to interact with the base layer components

The `digital_assets_layer` is a domain-level folder contains code related to the management of native Tari digital
assets. Substructure TBD.

It's envisaged that at least the following `applications` are built on top of the domain layer libraries:
* A standalone miner (`tari_miner`)
* A pool miner (`tari_pool_miner`)
* A CLI wallet for the Tari cryptocurrency (`cli_wallet`)
* A full node executable (`tari_basenode`)


