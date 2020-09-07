# Tari Merge Mining Proxy

## Setup (Mac)

These instructions assume you have a development environment already installed and configured.

These instructions will be updated to include the development environment setup.

### Tari Base Node
First we start tor by running the `start_tor.sh` script.

Then we need to run the Tari Base Node with `cargo run --bin tari_base_node -- --create_id` and then 
`cargo run --bin tari_base_node` 

Take note of the `grpc_address` in the config.toml file located in the `.tari` folder

### XMRig
Follow build instructions for XMRig here:
`https://xmrig.com/docs/miner/macos-build`

Then create a configuration for XMRig using the wizard here:
`https://xmrig.com/wizard`

When using the wizard you need to use `Add daemon` and the following public stagenet wallet address can be used:
`55LTR8KniP4LQGJSPtbYDacR7dz8RBFnsfAKMaMuwUNYX6aQbBcovzDPyrQF9KXF9tVU6Xk3K8no1BywnJX6GvZX8yJsXvt`

Take note of the `Host` and `Port` that was specified in the configuration.

You will put the resulting `config.json` file together with the built binaries for XMRig.

Start XMRig from the terminal using `./xmrig`

### Tari Merge Mining Proxy

In `main.rs` of the Tari merge mining proxy the following constants can be changed to reflect the above configuration:
```Rust
const MONEROD_URL: &str = "monero-stagenet.exan.tech:38081"; // To connect to the Monero Public Stagenet.
const TARI_GRPC_URL: &str = "http://127.0.0.1:18142"; // To connect to the Tari Base Node.
const LOCALHOST: &str = "127.0.0.1:7878"; // For XMRig to connect.
```

Then run the Tari Merge Mining Proxy with `cargo run --bin tari_merge_mining_proxy`.

Blocks that have been mined for Monero Stagenet can be viewed here:
`https://monero-stagenet.exan.tech/`
