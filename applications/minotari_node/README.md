# Minotari base node

# OSX
## Installation from binaries

If you've downloaded binaries from [the Tari homepage](https://tari.com/downloads), then installing the base node is
relatively simple.

1. Extract the contents of the zip archive to a convenient location (e.g. `/Users/your_name/tari_node`). Since you're
   reading this file, you've probably done this already.
2. Give the `minotari_node` executable permission to run on your machine. Right-click on the `minotari_node`
   executable in Finder, select `Open`, and then click `Open`. The node will exit with an error, but this is fine; all
   we wanted to do is tell your Mac that it's ok to run this program.
3. Run `install.sh` by double clicking on it, or entering `./install.sh` in a terminal.
4. When you see the node prompt, you're good to go!

```
>> help 
Available commands are: 
help, version, get-chain-metadata, list-peers, reset-offline-peers, ban-peer, unban-peer, list-connections, list-headers, 
check-db, calc-timing, discover-peer, get-block, search-utxo, search-kernel, search-stxo, get-mempool-stats, 
get-mempool-state, whoami, get-state-info, quit, exit
```



### Prerequisites

#### Linux
```
sudo apt-get install git curl build-essential cmake clang pkg-config libssl-dev libsqlite3-dev sqlite3
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

```
### From source

```
cargo install minotari_node
```

## Configuration
