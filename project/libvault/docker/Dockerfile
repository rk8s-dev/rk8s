# -----------------------------------------------------------------------------
# Build stage
# -----------------------------------------------------------------------------
FROM rustlang/rust:nightly-bookworm AS builder
WORKDIR /opt/build

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        pkg-config \
        libssl-dev \
        clang \
        protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

# Cache dependencies
COPY Cargo.toml ./
COPY build.rs ./
COPY src ./src
COPY bin ./bin
COPY tests ./tests
RUN cargo build --release --bin rvault

# -----------------------------------------------------------------------------
# Runtime stage
# -----------------------------------------------------------------------------
FROM ubuntu:22.04

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        jq \
        gosu \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --system --create-home --home /opt/rusty_vault --shell /usr/sbin/nologin rvault

WORKDIR /opt/rusty_vault

COPY --from=builder /opt/build/target/release/rvault /usr/local/bin/rvault
COPY docker/files/config.hcl /etc/rusty_vault/config.hcl
COPY docker/bootstrap_quic_pki.sh /usr/local/bin/bootstrap_quic_pki.sh
COPY docker/docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
COPY docker/quic_pki /opt/rusty_vault/quic_pki
COPY docker/scripts/create_rkl_bootstrap_token.sh /usr/local/bin/create_rkl_bootstrap_token.sh
COPY docker/scripts/create_rks_bootstrap_token.sh /usr/local/bin/create_rks_bootstrap_token.sh

RUN chmod +x /usr/local/bin/bootstrap_quic_pki.sh /usr/local/bin/docker-entrypoint.sh \
    /usr/local/bin/create_rkl_bootstrap_token.sh /usr/local/bin/create_rks_bootstrap_token.sh \
    && mkdir -p /opt/rusty_vault/state

ENV VAULT_ADDR=http://127.0.0.1:8200 \
    BOOTSTRAP_OUTPUT_DIR=/opt/rusty_vault/state

VOLUME ["/opt/rusty_vault/state"]

EXPOSE 8200

ENTRYPOINT ["/usr/local/bin/docker-entrypoint.sh"]
