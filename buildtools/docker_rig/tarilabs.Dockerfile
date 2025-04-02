# syntax = docker/dockerfile:1.3

# https://hub.docker.com/_/rust
ARG RUST_VERSION=1.84
ARG OS_BASE=bookworm

# rust source compile with cross platform build support
FROM --platform=$BUILDPLATFORM rust:$RUST_VERSION-${OS_BASE} as builder

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
ARG RUST_TARGET
ARG RUST_VERSION
ARG OS_BASE

ARG ARCH=native
#ARG FEATURES=avx2
ARG FEATURES="safe,grpc"

#ENV RUSTFLAGS="-C target_cpu=$ARCH"
#ENV ROARING_ARCH=$ARCH

ENV CARGO_HTTP_MULTIPLEXING=false

ARG VERSION=1.0.1
ARG APP_NAME=wallet
ARG APP_EXEC=minotari_console_wallet
ARG TARI_NETWORK
ARG TARI_TARGET_NETWORK

WORKDIR /tari

COPY Cargo.toml .
COPY Cargo.lock .
COPY rust-toolchain.toml .
COPY applications applications
COPY base_layer base_layer
COPY clients clients
COPY common common
COPY common_sqlite common_sqlite
COPY comms comms
COPY infrastructure infrastructure
COPY meta meta
COPY buildtools/deps_only buildtools/deps_only
COPY integration_tests integration_tests
COPY hashing hashing
COPY scripts scripts

# Disable Prompt During Packages Installation
ARG DEBIAN_FRONTEND=noninteractive

RUN apt-get update && \
    sh /tari/scripts/install_ubuntu_dependencies.sh

# Work around - Error: Not a git repository or CARGO_MANIFEST_DIR nested deeper than 10 from the root
RUN git init -q

RUN if [ "${BUILDARCH}" != "${TARGETARCH}" ] ; then \
      # Run script to help setup cross-compile environment
      . /tari/scripts/cross_compile_tooling.sh ; \
    fi

RUN if [ -n "${RUST_TOOLCHAIN}" ] ; then \
      # Install a non-standard toolchain if it has been requested.
      # By default we use the toolchain specified in rust-toolchain.toml
      rustup toolchain install ${RUST_TOOLCHAIN} --force-non-host ; \
    fi

# Breaking on arm64 builds
#RUN rustup show && \
#    rustup target list --installed && \
#    rustup toolchain list

RUN cargo build ${RUST_TARGET} \
      --bin ${APP_EXEC} --release --features ${FEATURES} --locked && \
    # Copy executable out of the cache so it is available in the runtime image.
    cp -v /tari/target/${BUILD_TARGET}release/${APP_EXEC} /tari/${APP_EXEC}

# Create runtime base minimal image for the target platform executables
FROM --platform=$TARGETPLATFORM bitnami/minideb:${OS_BASE} as runtime

ARG BUILDPLATFORM
ARG TARGETOS
ARG TARGETARCH
ARG TARGETVARIANT
ARG RUST_VERSION
ARG OS_BASE

ARG ARCH
ARG FEATURES

ARG VERSION

ARG APP_NAME
ARG APP_EXEC
ARG TARI_NETWORK
ARG TARI_TARGET_NETWORK

# Disable Prompt During Packages Installation
ARG DEBIAN_FRONTEND=noninteractive

RUN apt-get update && \
    apt-get install --no-install-recommends --assume-yes \
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
      procps \
      lsof && \
    # Docker image reduction
    apt-get clean all && \
    apt-get autoremove --assume-yes && \
    rm -rf /var/lib/apt/lists/* /tmp/* /var/tmp/*

RUN groupadd --gid 1000 tari && \
    useradd --create-home --no-log-init --shell /bin/bash \
      --home-dir /var/tari \
      --uid 1000 --gid 1000 tari

ENV dockerfile_version=$VERSION
ENV dockerfile_build_arch=$BUILDPLATFORM
ENV dockerfile_arch=$ARCH
ENV dockerfile_features=$FEATURES
ENV rust_version=$RUST_VERSION

ENV APP_NAME=${APP_NAME:-wallet}
ENV APP_EXEC=${APP_EXEC:-minotari_console_wallet}
ENV TARI_NETWORK=${TARI_NETWORK}
ENV TARI_TARGET_NETWORK=${TARI_TARGET_NETWORK}

RUN mkdir -p "/var/tari/${APP_NAME}" && \
    chown -R tari:tari "/var/tari/${APP_NAME}"

# Setup blockchain path for minotari_node only
RUN if [ "${APP_NAME}" = "node" ] ; then \
      echo "minotari_node bits" && \
      mkdir /blockchain && \
      chown -R tari:tari /blockchain && \
      chmod -R 0700 /blockchain ; \
    else \
      echo "Not minotari_node" ; \
    fi

USER tari

COPY --from=builder /tari/$APP_EXEC /usr/local/bin/
COPY buildtools/docker_rig/start_tari_app.sh /usr/local/bin/start_tari_app.sh

ENTRYPOINT [ "start_tari_app.sh" ]
CMD [ "--non-interactive-mode" ]
