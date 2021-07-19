#FROM rust:1.42.0 as builder
FROM quay.io/tarilabs/rust_tari-build-with-deps:nightly-2021-05-09 as builder

# Copy the dependency lists
#ADD Cargo.toml ./
ADD . /tari
WORKDIR /tari

# RUN rustup component add rustfmt --toolchain nightly-2020-08-13-x86_64-unknown-linux-gnu
#ARG WALLET_ARCH=native
ARG WALLET_ARCH=x86-64
#ARG WALLET_FEATURES=avx2
ENV RUSTFLAGS="-C target_cpu=$WALLET_ARCH"
ENV ROARING_ARCH=$WALLET_ARCH
# Work around for odd issue with broken Cargo.lock and builds
RUN cargo fetch && \
    cargo build --bin tari_console_wallet --locked --release

# Create a base minimal image for adding our executables to
FROM quay.io/bitnami/minideb:buster as base
# Disable Prompt During Packages Installation
ARG DEBIAN_FRONTEND=noninteractive
RUN apt update && apt -y install \
    apt-transport-https \
    bash \
    ca-certificates \
    curl \
    gpg \
    iputils-ping \
    less \
    libreadline7 \
    libreadline-dev \
    libsqlite3-0 \
    openssl

# Now create a new image with only the essentials and throw everything else away
FROM base

COPY --from=builder /tari/buildtools/docker_rig/start_wallet.sh /usr/bin/start_wallet.sh
COPY --from=builder /tari/target/release/tari_console_wallet /usr/bin/

ENV SHELL=/bin/bash
ENTRYPOINT ["start_wallet.sh", "-c", "/var/tari/config/config.toml", "-b", "/var/tari/wallet"]
