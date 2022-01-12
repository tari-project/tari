.PHONY: test

test:
	wasm-pack test --node --features js

.PHONY: build

build:
	wasm-pack build --target bundler . -- --features js

.PHONY: install

install:
	wasm-pack build --out-dir=../../applications/tari_web_extension/src/key_manager/ --target bundler . -- --features js

.PHONY: web

web:
	wasm-pack build --target web . -- --features js