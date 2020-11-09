# Tari Merge Mining Proxy

## Setup (Mac)

These instructions assume you have a development environment already installed and configured.

These instructions will be updated to include the development environment setup.

### Prerequisites

In the `config.toml` located in the `.tari` folder, there should be sections that looks similar to the following:
```
[base_node.ridcully]
db_type = "lmdb"
transport = "tor"
peer_seeds = []
enable_mining = false 
grpc_enabled = true
grpc_address = "127.0.0.1:18142"
grpc_wallet_address = "127.0.0.1:18143"

[merge_mining_proxy.ridcully]
monerod_url = "http://192.110.160.146:38081"
proxy_host_address = "127.0.0.1:7878"
# If authentication is being used for curl
monerod_use_auth=false
# Username for curl
monerod_username=""
# Password for curl
monerod_password=""
```

If these sections do not exist then they need to be added.

In order to merge mine successfully, the following settings are needed at a minimum:
1) enable_mining = false, this disables the SHA3 miner.
2) grpc_enabled = true, enables handling of grpc requests.
3) grpc_address has to be set to a valid address, the tari_base_node will handle grpc requests sent to this address.
4) grpc_wallet_address has to be set to a valid address, the tari_console_wallet will handle grpc requests sent to this address. 
5) proxy_host_address has to be set to a valid address, the tari_merge_mining_proxy will handle requests from XMRig sent to this address.
6) monerod_url has to be set to a valid address to monerod that is running monero stagenet. This can either be to one of the many public servers or to a local instance. 
Stagenet is usually served on port 38081 for monero, however it is best to consult the information on the site listing the stagenet server for the correct port.
A guide to setting up a local monero stagenet on Linux can be found [here](https://github.com/tari-project/tari/blob/development/applications/tari_merge_mining_proxy/monero_stagenet_setup.md).

### Tari Base Node

We need to start tor by running the `start_tor.sh` script.

Then we run the Tari Base Node. If it is the first run then use `cargo run --bin tari_base_node -- --create_id` followed by 
`cargo run --bin tari_base_node`. Otherwise just use `cargo run --bin tari_base_node`.

The value of `proxy_host_address` is needed later in this guide, make a note of it.

### Tari Console Wallet
We also have to start the console wallet:
`cargo run --bin tari_console_wallet`

### Tari Merge Mining Proxy
Now we run the Tari Merge Mining Proxy with `cargo run --bin tari_merge_mining_proxy`.

### XMRig
Follow build instructions for XMRig here:
`https://xmrig.com/docs/miner/macos-build`

Then create a configuration for XMRig using the wizard here:
`https://xmrig.com/wizard`

When using the wizard you need to use `Add daemon`. The following public stagenet wallet address can be used (or you can use your own stagenet wallet address),
`55LTR8KniP4LQGJSPtbYDacR7dz8RBFnsfAKMaMuwUNYX6aQbBcovzDPyrQF9KXF9tVU6Xk3K8no1BywnJX6GvZX8yJsXvt`. Just note that the address you supply here will be where the stagenet XMR will be paid into, 
so if you use the previous address you will not be able to access the XMR it holds (it is just a publicly available wallet address). 
The `Host` and `Port` fields can be filled in according to the value of `proxy_host_address` that was previously noted down.

Once you have finished the wizard, you will put the resulting `config.json` file together with the built binaries for XMRig.

Start XMRig from the terminal using `./xmrig`.

Alternatively, xmrig can also be run from the command line as follows (substitute monerod_url and stagenet_wallet_address with the values used previously):
```
./xmrig --donate-level 5 -o monerod_url -u stagenet_wallet_address --coin monero --daemon
```

