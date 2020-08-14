#FROM rust:1.42.0 as builder
FROM quay.io/tarilabs/rust_tari-build-with-deps:nightly-2020-06-10 as builder

# Copy the dependency lists
#ADD Cargo.toml ./
ADD . /tari_base_node
WORKDIR /tari_base_node

RUN rustup component add rustfmt --toolchain nightly-2020-06-10-x86_64-unknown-linux-gnu
#ARG TBN_ARCH=native
ARG TBN_ARCH=x86-64
#ARG TBN_FEATURES=avx2
ARG TBN_FEATURES=safe
ENV RUSTFLAGS="-C target_cpu=$TBN_ARCH"
ENV ROARING_ARCH=$TBN_ARCH
RUN cd applications/tari_base_node \
  && cargo build --release -p tari_base_node --features $TBN_FEATURES

# Create a base minimal image for adding our executables to
FROM bitnami/minideb:stretch as base
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
    ca-certificates && \
    # Add Sources for the latest tor
    printf "deb https://deb.torproject.org/torproject.org stretch main\ndeb-src https://deb.torproject.org/torproject.org stretch main" > /etc/apt/sources.list.d/tor.list && \
    curl https://deb.torproject.org/torproject.org/A3C4F0F979CAA22CDBA8F512EE8CBC9E886DDD89.asc | gpg --import 1>&2 && \
    gpg --export A3C4F0F979CAA22CDBA8F512EE8CBC9E886DDD89 | apt-key add - 1>&2 &&\
    apt update 1>&2 && \
    apt install -y tor deb.torproject.org-keyring 1>&2

# Now create a new image with only the essentials and throw everything else away
FROM base

COPY --from=builder /tari_base_node/target/release/tari_base_node /usr/bin/
COPY --from=builder /tari_base_node/common/config/tari_config_sample.toml /root/.tari/tari_config_sample.toml
COPY --from=builder /tari_base_node/common/logging/log4rs-sample.yml /root/.tari/log4rs.yml
COPY --from=builder /tari_base_node/buildtools/docker/torrc /etc/tor/torrc
COPY --from=builder /tari_base_node/buildtools/docker/start.sh /usr/bin/start.sh

# Keep the .tari directory in a volume by default
VOLUME ["/root/.tari"]
# Use start.sh to run tor then the base node or tari_base_node for the executable
CMD ["start.sh"]
