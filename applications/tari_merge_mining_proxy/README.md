# Tari Merge Mining Proxy

## Setup (Mac)

These instructions assume you have a development environment already installed and configured.

These instructions will be updated to include the development environment setup.

### Tari Base Node
First we start tor by running the `start_tor.sh` script.

Then we need to run the Tari Base Node. If it is the first run then use `cargo run --bin tari_base_node -- --create_id` followed by 
`cargo run --bin tari_base_node`. Otherwise just use `cargo run --bin tari_base_node`.

Take note of `proxy_host_address` in the config.toml file located in the `.tari` folder.

### Tari Console Wallet
Second is that we need to start the console wallet:
`cargo run --bin tari_console_wallet`

### XMRig
Follow build instructions for XMRig here:
`https://xmrig.com/docs/miner/macos-build`

Then create a configuration for XMRig using the wizard here:
`https://xmrig.com/wizard`

When using the wizard you need to use `Add daemon`. The following public stagenet wallet address can be used (or you can use your own stagenet wallet address):
`55LTR8KniP4LQGJSPtbYDacR7dz8RBFnsfAKMaMuwUNYX6aQbBcovzDPyrQF9KXF9tVU6Xk3K8no1BywnJX6GvZX8yJsXvt`. The `Host` and `Port` fields can be filled in according to the 
value of `proxy_host_address` that was previously noted down.

Once you have finished the wizard, you will put the resulting `config.json` file together with the built binaries for XMRig.

Start XMRig from the terminal using `./xmrig`.

### Tari Merge Mining Proxy
Then run the Tari Merge Mining Proxy with `cargo run --bin tari_merge_mining_proxy`.

The address of the stagenet daemon you are connecting to can be changed if needed in the `config.toml`, the relevant variable is `monerod_url`.


