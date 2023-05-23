# Tari Wallet

Foreign Function interface for the Tari Android and Tari iOS Wallets.

This crate is part of the [Tari Cryptocurrency](https://tari.com) project.

## Build setup (Mac)

See README.md in wallet_ffi crate

## Setup (Windows)

See README.md in wallet_ffi crate


## Running migrations:

- Ensure that you installed diesel with the sqlite feature flag:
  - `cargo install diesel_cli --no-default-features --features sqlite`
- If you updated the tables the following needs to be run from the `base_layer/wallet/` folder:
  - `diesel setup --database-url test.sqlite3`
  - `diesel migration run --database-url test.sqlite3`
 - After running this, make sure that the diesel update did not change BigInt to Integer in `schema.rs` (check for
   any unwanted changes)
