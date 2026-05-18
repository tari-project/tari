# Tari p2p examples

Examples for using the `tari_p2p` crate.

To run:

```bash
cargo run --example $name -- [args]
```

## C Dependencies

- [ncurses](https://github.com/mirror/ncurses)

---

## Examples

### [`gen_node_identity.rs`](gen_node_identity.rs)

Generates a random node identity JSON file. A node identity contains a node's public and secret keys, it's node id and
an address used to establish peer connections. The files generated from this example are used to populate the
peer manager in other examples.

```bash
cargo run --example gen_node_identity -- --help
cargo run --example gen_node_identity -- --output=examples/sample_identities/node-identity.json
```

### [`pingpong.rs`](pingpong.rs)

A basic ncurses UI that sends ping and receives pong messages to a single peer using the `tari_p2p` library.
Press 'p' to send a ping.

```bash
cargo run --example pingpong --features pingpong-example -- --help
cargo run --example pingpong --features pingpong-example -- --node-identity examples/sample_identities/node-identity1.json --peer-identity examples/sample_identities/node-identity2.json
```
