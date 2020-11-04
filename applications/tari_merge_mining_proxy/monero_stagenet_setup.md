# Monero - Simple Stagenet Environment Setup

## Linux

### Tools

```
sudo apt-get install net-tools
sudo apt-get install git
```

### Monero

1. Run the following in a terminal.
```
sudo apt update && sudo apt install build-essential cmake pkg-config libboost-all-dev libssl-dev libzmq3-dev libunbound-dev libsodium-dev libunwind8-dev liblzma-dev libreadline6-dev libldns-dev libexpat1-dev doxygen graphviz libpgm-dev qttools5-dev-tools libhidapi-dev libusb-1.0-0-dev libprotobuf-dev protobuf-compiler libudev-dev
git clone https://github.com/monero-project/monero.git
cd monero
git checkout release-v0.16
git git submodule init --force && git submodule update
make release
```
2. Rename release-v0.16 folder to master. The onion-monero-block-explorer looks for a master build path and will not configure correctly without it.

3. Enable huges pages. This is necessary for randomx validation)
```
sudo bash -c "echo vm.nr_hugepages=$(nproc) >> /etc/sysctl.conf"
```

4. Run MoneroD from the bin folder
```
./monerod --detach --stagenet --confirm-external-bind --rpc-bind-ip put.your.ip.here --add-exclusive-node 192.110.100.146
tail -f ~/.bitmonero/stagenet/bitmonero.log
```

5. Wait for the monero stagenet blockchain will to sync.

6. Run Monero CLI Wallet from the bin folder
```
monero-wallet-cli --stagenet
```

```

7. Create a new wallet thorugh the cli.

8. Set it to point to your local instance of MoneroD
```
set_daemon put.your.ip.here
```

9. Make a note of your wallet address and close the wallet.

10. Run Monero-Wallet-RPC from the bin folder
```
./monero-wallet-rpc --stagenet --confirm-external-bind --rpc-bind-port 38084 --daemon-host put.your.ip.here --wallet-file {path_to_created_wallet} --prompt-for-password
```

### Monero Pool

1. Run the following in a terminal
```
sudo apt-get install liblmdb-dev libevent-dev libjson-c-dev uuid-dev
export MONERO_ROOT=/path/to/cloned/monero
make release
```

2 Update the `pool.conf` configuration
In `pool.conf` the following settings need to be changed to:
```
pool-wallet = your_wallet_address
rpc-host = put.your.ip.here
rpc-port = 38081
wallet-rpc-host = 127.0.0.1
wallet-rpc-port = 38084
```

### Run the pool

1. To run the pool both `monerod` and `monero-wallet-rpc` need to be running.

2. Run the following in a terminal
```
./monero-pool --stagenet
```

### Onion Monero Blockchain Explorer

1. Run the following in a terminal
```
sudo apt-get install libcurl4-openssl-dev
sudo apt install curl
git clone https://github.com/moneroexamples/onion-monero-blockchain-explorer.git
cd onion-monero-blockchain-explorer
mkdir build && cd build
cmake .. -DMONERO_DIR=/path/to/cloned/monero
make
```

2. Run the explorer
```
./xmrblocks -s -b /path/to/monero/stagenet/lmdb/ -d put.your.ip.here --enable-randomx --enable-pusher --enable-emission-monitor --staenet-url 0.0.0.0:8081
```

### XMRig

1. Run the following in a terminal
```
sudo apt-get install git build-essential cmake libuv1-dev libssl-dev libhwloc-dev
git clone https://github.com/xmrig/xmrig.git
mkdir xmrig/build && cd xmrig/build
cmake ..
make -j$(nproc)
```

2. Follow the wizard tool here https://xmrig.com/wizard

When you get to the pools section, select "Add Daemon" and use your ip address for the host, 38081 for the port and your_wallet_address for Wallet Address.

Download the `json.config` and place it in the same directory as the XMRig exectable.

3. Run XMRig with
```
./xmrig
```
