FROM rustlang/rust:nightly-bookworm AS builder

RUN apt-get update && apt-get install -y \
    protobuf-compiler \
    libssl-dev \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

ENV PROTOC=/usr/bin/protoc
ENV PROTOC_INCLUDE=/usr/include

WORKDIR /workspace

# Copy workspace manifests first so dependency downloads can be cached.
COPY Cargo.toml Cargo.lock ./
COPY common/Cargo.toml common/Cargo.toml
COPY distribution/Cargo.toml distribution/Cargo.toml
COPY libbridge/Cargo.toml libbridge/Cargo.toml
COPY libcgroups/Cargo.toml libcgroups/Cargo.toml
COPY libcni/Cargo.toml libcni/Cargo.toml
COPY libcontainer/Cargo.toml libcontainer/Cargo.toml
COPY libfuse-fs/Cargo.toml libfuse-fs/Cargo.toml
COPY libipam/Cargo.toml libipam/Cargo.toml
COPY libnetwork/Cargo.toml libnetwork/Cargo.toml
COPY libscheduler/Cargo.toml libscheduler/Cargo.toml
COPY libvault/Cargo.toml libvault/Cargo.toml
COPY rfuse3/Cargo.toml rfuse3/Cargo.toml
COPY rkb/Cargo.toml rkb/Cargo.toml
COPY rkl/Cargo.toml rkl/Cargo.toml
COPY rks/Cargo.toml rks/Cargo.toml
COPY slayerfs/Cargo.toml slayerfs/Cargo.toml

# Now copy the full sources.
COPY common ./common
COPY distribution ./distribution
COPY libbridge ./libbridge
COPY libcgroups ./libcgroups
COPY libcni ./libcni
COPY libcontainer ./libcontainer
COPY libfuse-fs ./libfuse-fs
COPY libipam ./libipam
COPY libnetwork ./libnetwork
COPY libscheduler ./libscheduler
COPY libvault ./libvault
COPY rfuse3 ./rfuse3
COPY rkb ./rkb
COPY rkl ./rkl
COPY slayerfs ./slayerfs
COPY rks/src ./rks/src
COPY rks/tests ./rks/tests
COPY rks/BUCK ./rks/BUCK
COPY rks/readme.md ./rks/readme.md

RUN cargo build --release --locked -p rks

FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /opt/rks
ENV RKS_CONFIG=/opt/rks/config.yaml

COPY --from=builder /workspace/target/release/rks /usr/local/bin/rks
COPY rks/docker/entrypoint.sh /usr/local/bin/rks-entrypoint
COPY rks/docker/config.yaml /opt/rks/config.example.yaml
RUN chmod +x /usr/local/bin/rks-entrypoint

EXPOSE 50051/tcp 50051/udp

ENTRYPOINT ["/usr/local/bin/rks-entrypoint"]
CMD []
