FROM rust:1.88-bookworm

ENV DEBIAN_FRONTEND=noninteractive
ENV PATH="/usr/local/cargo/bin:/usr/local/rustup/bin:${PATH}"
RUN apt-get update && apt-get install -y --no-install-recommends \
    sudo ca-certificates git curl fuse3 redis-server \
    acl attr automake bc dbench dump e2fsprogs fio gawk gcc indent \
    libacl1-dev libaio-dev libcap-dev libgdbm-dev libtool libtool-bin \
    liburing-dev libuuid1 lvm2 make psmisc python3 quota sed uuid-dev \
    uuid-runtime xfsprogs exfatprogs f2fs-tools ocfs2-tools udftools \
    xfsdump xfslibs-dev protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /workspace/rk8s
