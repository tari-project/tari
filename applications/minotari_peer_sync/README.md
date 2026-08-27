# Minotari peer sync

A small diagnostic binary that does the base node's peer sync and nothing else.

It brings up the same comms + DHT stack the base node builds (`P2pInitializer` → comms → DHT network discovery), reads
the same `config.toml`, uses the same seed peers and DNS seeds, and lets the DHT run its normal seed strap: dial the
seed peers and stream their peer lists back into the peer database. Once that finishes it dials every peer it
downloaded and reports how many of them could be connected to.

No blockchain database, sync, mining or gRPC is started.

## Running

```bash
cargo run --release --bin minotari_peer_sync -- --network esmeralda
```

The binary is network-family locked at build time, exactly like the base node, so for mainnet build it with:

```bash
TARI_TARGET_NETWORK=mainnet cargo build --release --bin minotari_peer_sync
```

It reads `<base-path>/<network>/config/config.toml` — the same file and the same `[base_node]` / `[p2p.seeds]`
sections as `minotari_node` — so `--base-path`, `--config`, `--network` and `-p key=value` overrides all work the way
they do for the node. Example: force TCP instead of the configured tor transport with
`-p base_node.p2p.transport.type=tcp`.

Progress and the final report go to stdout; the detail (every seed sync and every dial) is logged to
`<base-path>/<network>/log/peer_sync/`.

## Safe to run next to a running base node

By default the tool:

- uses a **throw-away node identity**, so it does not claim the base node's node id on the network. It advertises no
  address of its own (peers accept an address-less identity claim), and it never expects inbound connections;
- writes to its **own peer database** in `<datastore_path>/peer_sync`, which is wiped at the start of every run so that
  peers really are downloaded each time. The base node's peer database is untouched;
- binds its listener to an **OS-assigned port** instead of the configured one.

`--use-node-identity` uses the configured identity and tor identity files instead. Do not use that while the base node
is running: two instances with the same node id interfere with each other, and loading the identity rewrites the
identity file.

## Options

| Option | Default | Description |
| --- | --- | --- |
| `--sync-timeout <secs>` | 180 | How long to wait for peer sync to complete |
| `--settle-time <secs>` | 5 | Grace period after peer sync before the peer list is read |
| `--dial-timeout <secs>` | 30 | Per-peer dial timeout |
| `--concurrency <n>` | 10 | How many peers to dial at once |
| `--max-peers <n>` | all | Only dial the first N peers |
| `--skip-seeds` | off | Do not dial the seed peers themselves |
| `--show-peers` | off | Print a line per peer with its address and result |
| `--reuse-peer-db` | off | Keep the peer database from the previous run |
| `--listener-port <port>` | 0 | Port to listen on |
| `--use-node-identity` | off | Use the base node's identity instead of a throw-away one |
| `--user-agent <string>` | base node's | User agent to advertise |

## Reading the report

```
================================ Peer sync report ================================
Network                       : esmeralda
Node id                       : 471dc6eab60a63071bd612049b (throw-away identity)
Transport                     : tcp
---------------------------------- Peer sync -----------------------------------
Seed peers (config + DNS)     : 2
Seed peers synced from        : 2
Peers downloaded              : 11
  new / duplicate this run    : 6 / 5
Peers in peer database        : 13
Peer sync status              : completed after 2.5s (1 round(s))
----------------------------------- Dialing ------------------------------------
Peers dialled                 : 13
Connected                     : 3 (23.1%)
Failed                        : 10
Time to dial all peers        : 2.2s
Failure reasons:
     10 x ConnectionFailed: All peer addresses are excluded for peer <peer>
================================================================================
```

- **Peers downloaded** is every non-seed peer in the peer database at the end of the sync. `new` and `duplicate` are
  what the seed strap round itself reported: two seeds handing out the same peer counts once as new and once as a
  duplicate.
- **Peer sync status** mirrors the base node's own early-exit rules — the DHT stops as soon as it has enough peers, so
  a short run with a modest peer count is normal, not a failure.
- `All peer addresses are excluded` above means the peer only advertises onion addresses while this run used the TCP
  transport. Use the tor transport (with tor running) to reach those peers.
