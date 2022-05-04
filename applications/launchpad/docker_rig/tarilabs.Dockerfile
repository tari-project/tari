# syntax = docker/dockerfile:1.3
#FROM quay.io/tarilabs/rust_tari-build-with-deps:nightly-2021-11-01 as builder

# Declare TARGETARCH to make it available
#ARG TARGETARCH

FROM rust:1.60-bullseye as builder

ARG TARGETOS
ARG TARGETARCH
ARG TARGETVARIANT

#RUN apt update && apt-get --no-install-recommends install -y \

# Disable anti-cache
RUN rm -f /etc/apt/apt.conf.d/docker-clean; echo 'Binary::apt::APT::Keep-Downloaded-Packages "true";' > /etc/apt/apt.conf.d/keep-cache
# https://github.com/moby/buildkit/blob/master/frontend/dockerfile/docs/syntax.md#run---mounttypecache
RUN --mount=type=cache,id=build-apt-cache-${TARGETOS}-${TARGETARCH}${TARGETVARIANT},sharing=locked,target=/var/cache/apt \
    --mount=type=cache,id=build-apt-lib-${TARGETOS}-${TARGETARCH}${TARGETVARIANT},sharing=locked,target=/var/lib/apt \
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

WORKDIR /tari

# Adding only necessary things up front and copying the entrypoint script last
# to take advantage of layer caching in docker
ADD Cargo.lock applications/deps_only/Cargo.lock
ADD rust-toolchain.toml .

#RUN pwd
#RUN ls
#RUN ls /
#RUN ls /tari

ARG ARCH=native
#ARG FEATURES=avx2
ARG FEATURES=safe
ENV RUSTFLAGS="-C target_cpu=$ARCH"
ENV ROARING_ARCH=$ARCH
ENV CARGO_HTTP_MULTIPLEXING=false

ARG APP_NAME=wallet
ARG APP_EXEC=tari_console_wallet

# Caches downloads across docker builds
ADD applications/deps_only applications/deps_only
WORKDIR applications/deps_only

#RUN --mount=type=cache,target=/usr/local/cargo/registry \
#    --mount=type=cache,target=/tari/target \
#    cargo build --bin deps_only --release

#RUN cargo build --bin deps_only --release

#RUN --mount=type=cache,id=rust-local-registry-${TARGETOS}-${TARGETARCH}${TARGETVARIANT},sharing=locked,target=/usr/local/cargo/registry \
#    --mount=type=cache,id=rust-target-${TARGETOS}-${TARGETARCH}${TARGETVARIANT},sharing=locked,target=/tari/target \

RUN --mount=type=cache,id=rust-git-${TARGETOS}-${TARGETARCH}${TARGETVARIANT},sharing=locked,target=/home/rust/.cargo/git \
    --mount=type=cache,id=rust-home-registry-${TARGETOS}-${TARGETARCH}${TARGETVARIANT},sharing=locked,target=/home/rust/.cargo/registry \
    --mount=type=cache,id=rust-local-registry-${TARGETOS}-${TARGETARCH}${TARGETVARIANT},sharing=locked,target=/usr/local/cargo/registry \
    --mount=type=cache,id=rust-src-target-${TARGETOS}-${TARGETARCH}${TARGETVARIANT},sharing=locked,target=/home/rust/src/target \
    --mount=type=cache,id=rust-target-${TARGETOS}-${TARGETARCH}${TARGETVARIANT},sharing=locked,target=/tari/target \
    cargo build --bin deps_only --release

#    --mount=type=cache,id=rust-app-deps-${TARGETOS}-${TARGETARCH}${TARGETVARIANT},sharing=locked,target=/tari/applications/deps_only \

WORKDIR /tari

ADD Cargo.toml .
ADD Cargo.lock .
ADD applications applications
ADD base_layer base_layer
ADD clients clients
ADD common common
ADD common_sqlite common_sqlite
ADD comms comms
ADD infrastructure infrastructure
ADD dan_layer dan_layer
ADD meta meta

#RUN cargo build --bin tari_console_wallet --release --features $FEATURES --locked

#RUN --mount=type=cache,sharing=private,target=/home/rust/.cargo/git \

#RUN --mount=type=cache,sharing=locked,target=/home/rust/.cargo/git \
#    --mount=type=cache,sharing=locked,target=/home/rust/.cargo/registry \
#    --mount=type=cache,sharing=locked,target=/usr/local/cargo/registry \
#    --mount=type=cache,sharing=locked,target=/tari/target \
#    --mount=type=cache,sharing=locked,target=/home/rust/src/target \
#    cargo build --bin ${APP_EXEC} --release --features $FEATURES --locked && \
#    # Copy executable out of the cache so it is available in the runtime image.
#    cp -v /tari/target/release/${APP_EXEC} /tari/${APP_EXEC}

#RUN cargo build --bin ${APP_EXEC} --release --features $FEATURES --locked

#RUN cargo build --bin ${APP_EXEC} --release --features $FEATURES --locked && \
#    # Copy executable out of the cache so it is available in the runtime image.
#    cp -v /tari/target/release/${APP_EXEC} /tari/${APP_EXEC}

RUN --mount=type=cache,id=rust-git-${TARGETOS}-${TARGETARCH}${TARGETVARIANT},sharing=locked,target=/home/rust/.cargo/git \
    --mount=type=cache,id=rust-home-registry-${TARGETOS}-${TARGETARCH}${TARGETVARIANT},sharing=locked,target=/home/rust/.cargo/registry \
    --mount=type=cache,id=rust-local-registry-${TARGETOS}-${TARGETARCH}${TARGETVARIANT},sharing=locked,target=/usr/local/cargo/registry \
    --mount=type=cache,id=rust-src-target-${TARGETOS}-${TARGETARCH}${TARGETVARIANT},sharing=locked,target=/home/rust/src/target \
    --mount=type=cache,id=rust-target-${TARGETOS}-${TARGETARCH}${TARGETVARIANT},sharing=locked,target=/tari/target \
    cargo build --bin ${APP_EXEC} --release --features $FEATURES --locked && \
    # Copy executable out of the cache so it is available in the runtime image.
    cp -v /tari/target/release/${APP_EXEC} /tari/${APP_EXEC}

#    --mount=type=cache,id=rust-app-deps-${TARGETOS}-${TARGETARCH}${TARGETVARIANT},sharing=locked,target=/tari/applications/deps_only \

# Create runtime base minimal image for the executables
#FROM quay.io/bitnami/minideb:bullseye as runtime
FROM bitnami/minideb:bullseye as runtime

ARG TARGETOS
ARG TARGETARCH
ARG TARGETVARIANT

ARG VERSION=1.0.1

ARG APP_NAME
ARG APP_EXEC

# Disable Prompt During Packages Installation
ARG DEBIAN_FRONTEND=noninteractive

#RUN apt update && apt-get --no-install-recommends install -y \

# Disable anti-cache
#RUN rm -f /etc/apt/apt.conf.d/docker-clean; echo 'Binary::apt::APT::Keep-Downloaded-Packages "true";' > /etc/apt/apt.conf.d/keep-cache
# https://github.com/moby/buildkit/blob/master/frontend/dockerfile/docs/syntax.md#run---mounttypecache
#RUN --mount=type=cache,sharing=locked,target=/var/cache/apt \
#    --mount=type=cache,sharing=locked,target=/var/lib/apt \

# Disable anti-cache
RUN rm -f /etc/apt/apt.conf.d/docker-clean; echo 'Binary::apt::APT::Keep-Downloaded-Packages "true";' > /etc/apt/apt.conf.d/keep-cache
# https://github.com/moby/buildkit/blob/master/frontend/dockerfile/docs/syntax.md#run---mounttypecache
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
ENV APP_NAME=${APP_NAME:-wallet}
ENV APP_EXEC=${APP_EXEC:-tari_console_wallet}

RUN mkdir -p "/var/tari/${APP_NAME}" \
    && chown -R tari.tari "/var/tari/${APP_NAME}"

#RUN mkdir /blockchain && chown tari.tari /blockchain && chmod 777 /blockchain
#RUN if [[ -z "$arg" ]] ; then echo Argument not provided ; else echo Argument is $arg ; fi
RUN if [[ "${APP_NAME}" == "base_node" ]] ; then \
      echo "Base_node bits" && \
      mkdir /blockchain && \
      chown -R tari.tari /blockchain && \
      chmod -R 0700 /blockchain ; \
    else \
      echo "Not base_node" ; \
    fi

USER tari

COPY --from=builder /tari/$APP_EXEC /usr/bin/
#COPY --from=builder /tari/target/release/$APP_EXEC /usr/bin/
COPY applications/launchpad/docker_rig/start_tari_app.sh /usr/bin/start_tari_app.sh

ENTRYPOINT [ "start_tari_app.sh", "-c", "/var/tari/config/config.toml", "-b", "/var/tari/${APP_NAME}" ]
# CMD [ "--non-interactive-mode" ]
