# NodeJS FFI Client

Still a work in progress.

## Install deps

- `npm install`

## Build FFI lib

- Build the FFI lib: `cargo build -p minotaiji_wallet_ffi --release --lib`
- Copy the lib into this folder: `cp target/release/libminotaiji_wallet_ffi.dylib /path/to/here`

_(.dylib for macOS, .so for Linux, .dll for windows)_

## Run

- `npm start` - runs index.js file
