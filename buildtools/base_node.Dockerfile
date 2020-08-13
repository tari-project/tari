#FROM rust:1.42.0 as builder
FROM quay.io/tarilabs/rust_tari-build-with-deps:nightly-2020-06-10 as builder

# Copy the dependency lists
#ADD Cargo.toml ./
ADD . /tari_base_node
WORKDIR /tari_base_node

RUN rustup component add rustfmt --toolchain nightly-2020-06-10-x86_64-unknown-linux-gnu
RUN cargo build --release

# Create a base minimal image for adding our executables to
FROM bitnami/minideb:stretch as base
RUN apt update && apt install \
    openssl \
    libsqlite3-0 \
    curl \
    bash \
    iputils-ping \
    less \
    telnet \
    gpg \
    apt-transport-https \
    ca-certificates \
    -y && \
    # Add Sources for the latest tor
    printf "deb https://deb.torproject.org/torproject.org stretch main\ndeb-src https://deb.torproject.org/torproject.org stretch main" > /etc/apt/sources.list.d/tor.list && \
    curl https://deb.torproject.org/torproject.org/A3C4F0F979CAA22CDBA8F512EE8CBC9E886DDD89.asc | gpg --import 1>&2 && \
    gpg --export A3C4F0F979CAA22CDBA8F512EE8CBC9E886DDD89 | apt-key add - 1>&2 &&\
    apt update 1>&2 && \
    apt install -y tor deb.torproject.org-keyring 1>&2 && \
    # Standard config for the tor client
    printf "SocksPort 127.0.0.1:9050\nControlPort 127.0.0.1:9051\nCookieAuthentication 0\nClientOnly 1\nClientUseIPv6 1" > /etc/tor/torrc && \
    # start.sh script to run tor, sleep 15 seconds and start the base node
    printf "#!/bin/bash\ntor &\nsleep 15\nif [[ ! -f ~/.tari/config.toml ]]; then\n  tari_base_node --init --create-id\nfi\ntari_base_node" > /usr/bin/start.sh && \
    chmod +x /usr/bin/start.sh


# Now create a new image with only the essentials and throw everything else away
FROM base

COPY --from=builder /tari_base_node/target/release/tari_base_node /usr/bin/
COPY --from=builder /tari_base_node/common/config/tari_config_sample.toml /root/.tari/tari_config_sample.toml
COPY --from=builder /tari_base_node/common/logging/log4rs-sample.yml /root/.tari/log4rs.yml

# Keep the .tari directory in a volume by default
VOLUME ["/root/.tari"]
# Use start.sh to run tor then the base node or tari_base_node for the executable
CMD ["start.sh"]
