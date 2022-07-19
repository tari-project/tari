# syntax = docker/dockerfile:1.3
FROM --platform=$BUILDPLATFORM rust:1.60-bullseye as builder

# Declare to make available
ARG BUILDPLATFORM
ARG BUILDOS
ARG BUILDARCH
ARG BUILDVARIANT
ARG TARGETPLATFORM
ARG TARGETOS
ARG TARGETARCH
ARG TARGETVARIANT
ARG RUST_TOOLCHAIN

# Disable anti-cache
RUN rm -f /etc/apt/apt.conf.d/docker-clean; echo 'Binary::apt::APT::Keep-Downloaded-Packages "true";' > /etc/apt/apt.conf.d/keep-cache
# https://github.com/moby/buildkit/blob/master/frontend/dockerfile/docs/syntax.md#run---mounttypecache
RUN --mount=type=cache,id=build-apt-cache-${BUILDOS}-${BUILDARCH}${BUILDVARIANT},sharing=locked,target=/var/cache/apt \
    --mount=type=cache,id=build-apt-lib-${BUILDOS}-${BUILDARCH}${BUILDVARIANT},sharing=locked,target=/var/lib/apt \
  apt update && apt-get install -y \
  apt-transport-https \
  bash \
  ca-certificates \
  curl \
  gpg \
  iputils-ping \
  less \
  libreadline-dev \
  libsqlite3-0 \
  openssl \
  telnet \
  cargo \
  clang \
  cmake

ARG ARCH=native
#ARG FEATURES=avx2
ARG FEATURES=safe
ENV RUSTFLAGS="-C target_cpu=$ARCH"
ENV ROARING_ARCH=$ARCH
ENV CARGO_HTTP_MULTIPLEXING=false

ARG APP_NAME=wallet
ARG APP_EXEC=tari_console_wallet

# GNU C compiler for the arm64 architecture and GNU C++ compiler
#RUN if [[ "${TARGETPLATFORM}" == "linux/arm64" ]] ; then \
RUN if [ "${TARGETARCH}" = "arm64" ] ; then \
      echo "Setup ARM64" && \
      apt update && \
      apt-get install -y gcc-aarch64-linux-gnu g++-aarch64-linux-gnu && \
      rustup target add aarch64-unknown-linux-gnu && \
      rustup toolchain install stable-aarch64-unknown-linux-gnu ; \
    else \
      echo "Setup x86-64" ; \
    fi

# Install a non-standard toolchain if it has been requested. By default we use the toolchain specified in rust-toolchain.toml
RUN if [ -n "${RUST_TOOLCHAIN}" ]; then \
    rustup toolchain install ${RUST_TOOLCHAIN}; \
    fi

WORKDIR /tari

ADD Cargo.toml .
ADD Cargo.lock .
ADD rust-toolchain.toml .
ADD applications applications
ADD base_layer base_layer
ADD clients clients
ADD common common
ADD common_sqlite common_sqlite
ADD comms comms
ADD infrastructure infrastructure
ADD dan_layer dan_layer
ADD meta meta

RUN --mount=type=cache,id=rust-git-${TARGETOS}-${TARGETARCH}${TARGETVARIANT},sharing=locked,target=/home/rust/.cargo/git \
    --mount=type=cache,id=rust-home-registry-${TARGETOS}-${TARGETARCH}${TARGETVARIANT},sharing=locked,target=/home/rust/.cargo/registry \
    --mount=type=cache,id=rust-local-registry-${TARGETOS}-${TARGETARCH}${TARGETVARIANT},sharing=locked,target=/usr/local/cargo/registry \
    --mount=type=cache,id=rust-src-target-${TARGETOS}-${TARGETARCH}${TARGETVARIANT},sharing=locked,target=/home/rust/src/target \
    --mount=type=cache,id=rust-target-${TARGETOS}-${TARGETARCH}${TARGETVARIANT},sharing=locked,target=/tari/target \
    if [ "${TARGETARCH}" = "arm64" ] ; then \
      export BUILD_TARGET="aarch64-unknown-linux-gnu/" && \
      export RUST_TARGET="--target=aarch64-unknown-linux-gnu" && \
      export ARCH=generic && \
      export FEATURES=safe && \
      export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc && \
      export CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc && \
      export CXX_aarch64_unknown_linux_gnu=aarch64-linux-gnu-g++ && \
      export BINDGEN_EXTRA_CLANG_ARGS="--sysroot /usr/aarch64-linux-gnu/include/" && \
      export RUSTFLAGS="-C target_cpu=generic" && \
      export ROARING_ARCH=generic ; \
    fi && \
    if [ -n "${RUST_TOOLCHAIN}" ]; then \
      export toolchain=+${RUST_TOOLCHAIN}; \
    fi && \
    cargo update && \
    cargo ${toolchain} build ${RUST_TARGET} \
      --bin ${APP_EXEC} --release --features $FEATURES --locked && \
    # Copy executable out of the cache so it is available in the runtime image.
    cp -v /tari/target/${BUILD_TARGET}release/${APP_EXEC} /tari/${APP_EXEC}

# Create runtime base minimal image for the target platform executables
FROM --platform=$TARGETPLATFORM bitnami/minideb:bullseye as runtime

ARG BUILDPLATFORM
ARG TARGETOS
ARG TARGETARCH
ARG TARGETVARIANT

ARG VERSION=1.0.1

ARG APP_NAME
ARG APP_EXEC

# Disable Prompt During Packages Installation
ARG DEBIAN_FRONTEND=noninteractive

# Disable anti-cache
RUN rm -f /etc/apt/apt.conf.d/docker-clean; echo 'Binary::apt::APT::Keep-Downloaded-Packages "true";' > /etc/apt/apt.conf.d/keep-cache
RUN --mount=type=cache,id=runtime-apt-cache-${TARGETOS}-${TARGETARCH}${TARGETVARIANT},sharing=locked,target=/var/cache/apt \
    --mount=type=cache,id=runtime-apt-lib-${TARGETOS}-${TARGETARCH}${TARGETVARIANT},sharing=locked,target=/var/lib/apt \
  apt update && apt-get --no-install-recommends install -y \
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

ENV dockerfile_version=$VERSION
ENV dockerfile_build_arch=$BUILDPLATFORM
ENV APP_NAME=${APP_NAME:-wallet}
ENV APP_EXEC=${APP_EXEC:-tari_console_wallet}

RUN mkdir -p "/var/tari/${APP_NAME}" && \
    chown -R tari.tari "/var/tari/${APP_NAME}"

# Setup blockchain path for base_node only
RUN if [ "${APP_NAME}" = "base_node" ] ; then \
      echo "Base_node bits" && \
      mkdir /blockchain && \
      chown -R tari.tari /blockchain && \
      chmod -R 0700 /blockchain ; \
    else \
      echo "Not base_node" ; \
    fi

USER tari

COPY --from=builder /tari/$APP_EXEC /usr/bin/
COPY applications/launchpad/docker_rig/start_tari_app.sh /usr/bin/start_tari_app.sh

ENTRYPOINT [ "start_tari_app.sh", "-c", "/var/tari/config/config.toml", "-b", "/var/tari/${APP_NAME}" ]
# CMD [ "--non-interactive-mode" ]
