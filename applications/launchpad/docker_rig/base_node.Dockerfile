FROM quay.io/tarilabs/rust_tari-build-with-deps:nightly-2021-09-18 as builder

WORKDIR /tari

# Adding only necessary things up front and copying the entrypoint script last
# to take advantage of layer caching in docker
ADD Cargo.lock .
ADD Cargo.toml .
ADD applications applications
ADD base_layer base_layer
ADD common common
ADD comms comms
ADD infrastructure infrastructure
ADD meta meta
ADD rust-toolchain.toml .

ARG ARCH=native
ARG FEATURES=avx2
ENV RUSTFLAGS="-C target_cpu=$ARCH"
ENV ROARING_ARCH=$ARCH
ENV CARGO_HTTP_MULTIPLEXING=false

# Caches downloads across docker builds
RUN cargo build --bin deps_only --release

RUN cargo build --bin tari_base_node --release --features $FEATURES --locked

# Create a base minimal image for the executables
FROM quay.io/bitnami/minideb:bullseye as base
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
  libreadline8 \
  libreadline-dev \
  libsqlite3-0 \
  openssl \
  telnet

RUN groupadd -g 1000 tari && useradd -s /bin/bash -u 1000 -g 1000 tari

RUN mkdir -p "/var/tari/base_node/weatherwax" \
    && mkdir -p "/var/tari/base_node/igor" \
    && mkdir -p "/var/tari/base_node/mainnet" \
    && chown -R tari.tari "/var/tari/base_node"

USER tari

ENV APP_NAME=base_node APP_EXEC=tari_base_node

COPY --from=builder /tari/target/release/$APP_EXEC /usr/bin/
COPY applications/launchpad/docker_rig/start_tari_app.sh /usr/bin/start_tari_app.sh

ENTRYPOINT [ "start_tari_app.sh", "-c", "/var/tari/config/config.toml", "-b", "/var/tari/base_node" ]
# CMD [ "--non-interactive-mode" ]
