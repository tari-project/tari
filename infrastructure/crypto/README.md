# Tari Crypto

This crate is part of the [Tari Cryptocurrency](https://tari.com) project.

Major features of this library include:

* Pedersen commitments
* Schnorr Signatures
* Generic Public and Secret Keys
* [Musig!](https://blockstream.com/2018/01/23/musig-key-aggregation-schnorr-signatures/)

The `tari_crypto` crate makes heavy use of the excellent [Dalek](https://github.com/dalek-cryptography/curve25519-dalek)
libraries. The default implementation for Tari ECC is the [Ristretto255 curve](https://ristretto.group).

# Compiling to WebAssembly
To build the WebAssembly module, the `wasm` feature must be enabled:

    $ wasm-pack build . -- --features "wasm"

To generate a module for use in node.js, use this command:

    $ wasm-pack build --target nodejs -d tari_js . -- --features "wasm"

## Example (Node.js)

```js
const keys = KeyRing.new();

// Create new keypair
keys.new_key("Alice");
keys.new_key("Bob");
console.log(`${keys.len()} keys in ring`); // 2
console.log("kA = ", keys.private_key("Alice"));
console.log("PB = ", keys.public_key("Bob"));
keys.free();
````

# Benchmarks

To run the benchmarks:

    $ cargo bench

The benchmarks use Criterion and will produce nice graphs (if you have gnuplot installed)

To run the benchmarks with SIMD instructions:

    $ cargo bench --features "avx2"