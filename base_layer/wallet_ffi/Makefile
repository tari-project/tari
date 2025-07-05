.PHONY: help clean build build-debug build-release test test-rust test-python fmt lint check develop install-dev wheel docs

# Default target
help:
	@echo "Tari Wallet FFI Development Commands"
	@echo ""
	@echo "Building:"
	@echo "  build         Build in release mode"
	@echo "  build-debug   Build in debug mode"
	@echo "  build-release Build in release mode"
	@echo "  develop       Build and install for development"
	@echo "  wheel         Build Python wheel"
	@echo ""
	@echo "Testing:"
	@echo "  test          Run all tests"
	@echo "  test-rust     Run Rust tests only"
	@echo "  test-python   Run Python tests only"
	@echo ""
	@echo "Code Quality:"
	@echo "  fmt           Format code (Rust and Python)"
	@echo "  lint          Run linting (clippy and ruff)"
	@echo "  check         Run all quality checks"
	@echo ""
	@echo "Development:"
	@echo "  install-dev   Install development dependencies"
	@echo "  clean         Clean build artifacts"
	@echo ""
	@echo "Documentation:"
	@echo "  docs          Build documentation"

# Environment
CARGO_FLAGS := --features python-bindings
PYTEST_FLAGS := -v
RUST_LOG := debug
TARI_TARGET_NETWORK := nextnet

# Building
build: build-release

build-debug:
	@echo "Building in debug mode..."
	cargo build $(CARGO_FLAGS)

build-release:
	@echo "Building in release mode..."
	cargo build --release $(CARGO_FLAGS)

develop:
	@echo "Building for development..."
	maturin develop --release $(CARGO_FLAGS)

wheel:
	@echo "Building Python wheel..."
	maturin build --release $(CARGO_FLAGS)

# Testing
test: test-rust test-python

test-rust:
	@echo "Running Rust tests..."
	RUST_LOG=$(RUST_LOG) TARI_TARGET_NETWORK=$(TARI_TARGET_NETWORK) \
	cargo test $(CARGO_FLAGS)

test-python:
	@echo "Running Python tests..."
	TARI_TARGET_NETWORK=$(TARI_TARGET_NETWORK) \
	python -m pytest tests/ $(PYTEST_FLAGS)

# Code Quality
fmt:
	@echo "Formatting Rust code..."
	cargo fmt
	@echo "Formatting Python code..."
	black python/ tests/ examples/
	isort python/ tests/ examples/

lint:
	@echo "Running Rust linting..."
	cargo clippy $(CARGO_FLAGS) -- -D warnings
	@echo "Running Python linting..."
	ruff check python/ tests/ examples/
	mypy python/ --ignore-missing-imports

check: fmt lint test
	@echo "All quality checks passed!"

# Development
install-dev:
	@echo "Installing development dependencies..."
	pip install -r requirements-dev.txt
	pip install maturin

clean:
	@echo "Cleaning build artifacts..."
	cargo clean
	rm -rf target/wheels
	rm -rf python/tari_wallet/__pycache__
	rm -rf tests/__pycache__
	find . -name "*.pyc" -delete
	find . -name "__pycache__" -type d -exec rm -rf {} + 2>/dev/null || true

# Documentation
docs:
	@echo "Building Rust documentation..."
	cargo doc --features python-bindings --open

# Quick development cycle
dev: clean build-debug test-python
	@echo "Development cycle complete!"

# CI-like checks
ci: clean fmt lint test wheel
	@echo "CI checks complete!"
