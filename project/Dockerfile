FROM rust:1-slim AS builder

ARG PROFILE=release

RUN cat <<'EOF' >/etc/apt/sources.list 
deb https://mirrors.tuna.tsinghua.edu.cn/debian/ trixie main contrib non-free non-free-firmware
# deb-src https://mirrors.tuna.tsinghua.edu.cn/debian/ trixie main contrib non-free non-free-firmware

deb https://mirrors.tuna.tsinghua.edu.cn/debian/ trixie-updates main contrib non-free non-free-firmware
# deb-src https://mirrors.tuna.tsinghua.edu.cn/debian/ trixie-updates main contrib non-free non-free-firmware

deb https://mirrors.tuna.tsinghua.edu.cn/debian/ trixie-backports main contrib non-free non-free-firmware
# deb-src https://mirrors.tuna.tsinghua.edu.cn/debian/ trixie-backports main contrib non-free non-free-firmware
deb https://mirrors.tuna.tsinghua.edu.cn/debian-security trixie-security main contrib non-free non-free-firmware
# deb-src https://mirrors.tuna.tsinghua.edu.cn/debian-security trixie-security main contrib non-free non-free-firmware
EOF

RUN mkdir -p /root/.cargo && cat <<'EOF' >/root/.cargo/config.toml
[source.crates-io]
replace-with = 'rsproxy-sparse'

[source.rsproxy]
registry = "https://rsproxy.cn/crates.io-index"

[source.rsproxy-sparse]
registry = "sparse+https://rsproxy.cn/index/"

[registries.rsproxy]
index = "https://rsproxy.cn/crates.io-index"

[net]
git-fetch-with-cli = true
EOF


WORKDIR /usr/src/rk8s

COPY . .


# install necessary packages 
RUN apt-get update && apt-get install --fix-missing -y \
    build-essential \
    clang \
    lld \
    pkg-config \
    protobuf-compiler \
    seccomp \
    libseccomp-dev \
    libpython3-dev \
    openssl \
    libssl-dev \
    zstd

# start to build
RUN cargo build --release -p rks -p libipam -p libnetwork -p libbridge 

# ubuntu:24.04
FROM ubuntu@sha256:7c06e91f61fa88c08cc74f7e1b7c69ae24910d745357e0dfe1d2c0322aaf20f9
ARG ARCH=x86_64


RUN cat <<'EOF' >/etc/apt/sources.list 
deb https://mirrors.tuna.tsinghua.edu.cn/ubuntu/ noble main restricted universe multiverse
# deb-src https://mirrors.tuna.tsinghua.edu.cn/ubuntu/ noble main restricted universe multiverse
deb https://mirrors.tuna.tsinghua.edu.cn/ubuntu/ noble-updates main restricted universe multiverse
# deb-src https://mirrors.tuna.tsinghua.edu.cn/ubuntu/ noble-updates main restricted universe multiverse
deb https://mirrors.tuna.tsinghua.edu.cn/ubuntu/ noble-backports main restricted universe multiverse
# deb-src https://mirrors.tuna.tsinghua.edu.cn/ubuntu/ noble-backports main restricted universe multiverse

deb https://mirrors.tuna.tsinghua.edu.cn/ubuntu/ noble-security main restricted universe multiverse
deb-src https://mirrors.tuna.tsinghua.edu.cn/ubuntu/ noble-security main restricted universe multiverse
EOF

RUN apt-get update && \
    apt-get install --fix-missing -y --no-install-recommends \
    wget \
    curl \
    iproute2 \
    iptables \
    ipset \
    net-tools \
    dnsutils \
    iputils-ping \
    traceroute \
    tcpdump \
    ethtool \
    netcat-openbsd \
    procps \
    psmisc \
    lsof \
    vim \
    less \
    tar \
    gzip \
    unzip \
    file \
    sudo \
    ca-certificates && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*


WORKDIR /root

RUN mkdir -p /etc/net.d

COPY --from=builder /usr/src/rk8s/target/release/libbridge /opt/cni/bin
COPY --from=builder /usr/src/rk8s/target/release/libipam /opt/cni/bin
COPY --from=builder /usr/src/rk8s/target/release/libnetwork /opt/cni/bin
COPY --from=builder /usr/src/rk8s/target/release/rks /usr/bin/rks
COPY --from=builder /usr/src/rk8s/test/test.conflist /etc/net.d
COPY --from=builder /usr/src/rk8s/test/subnet.env /etc/net.d