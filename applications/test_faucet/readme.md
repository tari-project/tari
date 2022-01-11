# testnet faucet utxo generator

- To build the generator in release mode with avx2 enabled, run this command from the root folder of the tari repo:

```bash
RUSTFLAGS='-C target-feature=+avx2' cargo build --bin test_faucet --release --features avx2
```

- Copy the generated `test_faucet` binary from `target/release` to another folder
- Navigate to that folder and execute the binary: `./test_faucet`
- You should now have these additional files: `utxos.json` and `keys.json`

- go to `print_new_genesis_block` test and check network is correct, then run the test to print the new raw block:

```
cargo test --package tari_core --test mempool -- helpers::block_builders::print_new_genesis_block --exact --nocapture --ignored
```
