#FROM rust:1.42.0 as builder
FROM quay.io/tarilabs/rust_tari-build-with-deps:nightly-2020-01-08 as builder

# Copy the dependency lists
#ADD Cargo.toml ./
ADD . /tari_base_node
WORKDIR /tari_base_node

RUN rustup component add rustfmt --toolchain nightly-2020-01-08-x86_64-unknown-linux-gnu
RUN cargo build -p tari_base_node --release

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
    -y

# Now create a new image with only the essentials and throw everything else away
FROM base
COPY --from=builder /tari_base_node/target/release/tari_base_node /usr/bin/
#COPY --from=builder /basenode/target/release/tari_base_node.d /etc/tari_base_node.d
RUN mkdir /etc/tari_base_node.d
COPY --from=builder /tari_base_node/common/config/tari_config_sample.toml /etc/tari_base_node.d/tari_config_sample.toml
COPY --from=builder /tari_base_node/common/logging/log4rs-sample.yml /root/.tari/log4rs.yml

CMD ["tari_base_node"]
