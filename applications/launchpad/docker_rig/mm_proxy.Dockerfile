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

# Cusomised build
RUN cargo build --bin tari_merge_mining_proxy --release --locked --features "$FEATURES"

# Create a base minimal image for the executables
FROM quay.io/bitnami/minideb:bullseye as base
# Disable Prompt During Packages Installation
ARG DEBIAN_FRONTEND=noninteractive
RUN apt update && apt -y install \
  apt-transport-https \
  bash \
  ca-certificates \
  curl \
  openssl

# Now create a new image with only the essentials and throw everything else away
FROM base
RUN groupadd -g 1000 tari && useradd -s /bin/bash -u 1000 -g 1000 tari
USER tari

ENV APP_NAME=mm_proxy APP_EXEC=tari_merge_mining_proxy

COPY --from=builder /tari/target/release/$APP_EXEC /usr/bin/
COPY applications/launchpad/docker_rig/start_tari_app.sh /usr/bin/start_tari_app.sh

ENTRYPOINT [ "start_tari_app.sh", "-c", "/var/tari/config/config.toml", "-b", "/var/tari/mm_proxy" ]
# CMD [ "--non-interactive-mode" ]
