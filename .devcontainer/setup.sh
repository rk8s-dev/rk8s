## update and install some things we should probably have
apt-get update
apt-get install -y \
  curl \
  git \
  gnupg2 \
  jq \
  sudo \
  zsh \
  build-essential \
  openssl \
  libssl-dev \
  fuse3 \
  libfuse3-dev \
  pkg-config \
  cmake \
  clang \
  wget \
  file \
  ca-certificates \
  zstd

## Install rustup and common components
curl https://sh.rustup.rs -sSf | sh -s -- -y
rustup install default
rustup component add rustfmt
rustup component add clippy

cargo install cargo-expand
cargo install cargo-edit

## Install Buck2
wget https://github.com/facebook/buck2/releases/download/2025-02-01/buck2-x86_64-unknown-linux-musl.zst
zstd -d /home/buck2-x86_64-unknown-linux-musl.zst
mv /home/buck2-x86_64-unknown-linux-musl /home/buck2
chmod +x /home/buck2
mv /home/buck2 /usr/local/bin/buck2
