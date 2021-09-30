FROM quay.io/tarilabs/rust_tari-build-with-deps:nightly-2021-08-17 as builder

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
ADD rust-toolchain .

# RUN rustup component add rustfmt --toolchain nightly-2020-08-13-x86_64-unknown-linux-gnu
#ARG TBN_ARCH=native
ARG TBN_ARCH=x86-64
#ARG TBN_FEATURES=avx2
ARG TBN_FEATURES=safe
ENV RUSTFLAGS="-C target_cpu=$TBN_ARCH"
ENV ROARING_ARCH=$TBN_ARCH

RUN cargo build --bin tari_console_wallet --release --features $TBN_FEATURES --locked

# Create a base minimal image for the executables
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
    openssl \
    telnet

# Now create a new image with only the essentials and throw everything else away
FROM base
ENV APP_NAME=wallet APP_EXEC=tari_console_wallet

COPY --from=builder /tari/target/release/$APP_EXEC /usr/bin/
COPY buildtools/docker_rig/start_wallet.sh /usr/bin/start_tari_app.sh

ENTRYPOINT [ "start_tari_app.sh", "-c", "/var/tari/config/config.toml", "-b", "/var/tari/wallet" ]
# CMD [ "--non-interactive-mode" ]
