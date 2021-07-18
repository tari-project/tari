#FROM rust:1.42.0 as builder
FROM quay.io/tarilabs/rust_tari-build-with-deps:nightly-2021-05-09 as builder

# Copy the dependency lists
#ADD Cargo.toml ./
ADD . /tari
WORKDIR /tari

# RUN rustup component add rustfmt --toolchain nightly-2020-08-13-x86_64-unknown-linux-gnu
#ARG TBN_ARCH=native
ARG TBN_ARCH=x86-64
#ARG TBN_FEATURES=avx2
ARG TBN_FEATURES=safe
ENV RUSTFLAGS="-C target_cpu=$TBN_ARCH"
ENV ROARING_ARCH=$TBN_ARCH
# Work around for odd issue with broken Cargo.lock and builds
RUN cargo fetch && \
  cargo build --bin tari_base_node --release --features $TBN_FEATURES --locked

# Create a base minimal image for adding our executables to
FROM quay.io/bitnami/minideb:buster as base
# Disable Prompt During Packages Installation
ARG DEBIAN_FRONTEND=noninteractive
RUN apt update && apt -y install \
    openssl \
    libsqlite3-0 \
    curl \
    bash \
    iputils-ping \
    less \
    telnet \
    gpg \
    apt-transport-https \
    ca-certificates

# Now create a new image with only the essentials and throw everything else away
FROM base

COPY --from=builder /tari/target/release/tari_base_node /usr/bin/
COPY --from=builder /tari/buildtools/docker/start_base_node.sh /usr/bin/start_base_node.sh

# Use start.sh to run tor then the base node or tari_base_node for the executable
CMD ["start_base_node.sh", "-c", "/var/tari/config/config.toml", "-b", "/var/tari/base_node"]
