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
ENV APP_NAME=base_node APP_EXEC=tari_base_node

COPY --from=builder /tari/target/release/$APP_EXEC /usr/bin/
COPY --from=builder /tari/buildtools/docker_rig/start_tari_app.sh /usr/bin/start_tari_app.sh


ENTRYPOINT [ "start_tari_app.sh", "-c", "/var/tari/config/config.toml", "-b", "/var/tari/base_node" ]
CMD [ "-d" ]
