.DEFAULT_GOAL := test

.phony: build
build:
	cargo build --release --bin tari_base_node && \
	cargo build --release --bin tari_console_wallet && \
	cargo build --release --bin tari_mining_node && \
	cargo build --release --bin tari_merge_mining_proxy && \
	cargo build --release --package tari_wallet_ffi

.phony: test
test: build
	npm test -- --profile "ci" --tags "@critical and not @long-running and not @broken and not @wallet-ffi"
