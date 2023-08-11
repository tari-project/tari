toolchain=nightly-2023-06-03

.phony: test
test:
	rustup run $(toolchain) wasm-pack test --node --features wasm

.phony: build
build:
	rustup run $(toolchain) wasm-pack build --target bundler . -- --features wasm

.phony: install
install:
	rustup run $(toolchain) wasm-pack build --out-dir=../../applications/tari_web_extension/src/key_manager/ --target bundler . -- --features wasm

.phony: web
web:
	rustup run $(toolchain) wasm-pack build --target web . -- --features wasm
