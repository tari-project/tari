# Standalone miner for the Tari network

Miner application should be running connected to base node and console wallet allowing to search and create blocks
matching network difficulty, attaching wallet's provided coinbase. 

This also suits as reference implementation for custom miners which can be derived from it.


### Installation

Requires running console wallet and base node, please refer to relevant for instructions.
Requires rust toolchain, which can be setup via [rustup](https://rustup.rs/).

To run locally clone git repo and run following command:
```
cargo run --release --bin tari_mining_node
```

### Configuration

When running with local base node and console wallet it should work with zero-configuration.

Miner node is managed through Tari's `config.toml` file under `[mining_node]` subsection, supporting following parameters:
 - `base_node_grpc_address` - is IPv4/IPv6 address including port number, by which Tari Base Node can be found
 - `wallet_grpc_address` - is IPv4/IPv6 address including port number, where Tari Wallet Node can be found
 - `num_mining_threads` - number of mining threads, defaults to number of cpu cores
 - `mine_on_tip_only` - will start mining only when node is reporting bootstrapped state
 - `validate_tip_timeout_sec` - will check tip with node every N seconds and restart mining
 if current template height is taken

### Caveats 

Currently it supports only Sha3 mining which is suitable for TestNet.

