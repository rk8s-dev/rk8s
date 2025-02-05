## update and install some things we should probably have
apt-get update
apt-get install -y \
  curl \
  git \
  gnupg2 \
  jq \
  sudo \
  zsh \
  vim \
  build-essential \
  openssl \
  libssl-dev \
  fuse3 \
  libfuse3-dev \
  pkg-config \
  postgresql \
  cmake \
  clang \
  nodejs \
  npm \
  wget \
  file \
  libgtk-3-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  ca-certificates \
  zstd

## Install rustup and common components
curl https://sh.rustup.rs -sSf | sh -s -- -y
rustup install default
rustup component add rustfmt
rustup component add clippy

cargo install cargo-expand
cargo install cargo-edit

## Setup and install oh-my-zsh
sh -c "$(curl -fsSL https://raw.githubusercontent.com/robbyrussell/oh-my-zsh/master/tools/install.sh)"
cp -R /root/.oh-my-zsh /home/$USERNAME
cp /root/.zshrc /home/$USERNAME
sed -i -e "s/\/root\/.oh-my-zsh/\/home\/$USERNAME\/.oh-my-zsh/g" /home/$USERNAME/.zshrc
chown -R $USER_UID:$USER_GID /home/$USERNAME/.oh-my-zsh /home/$USERNAME/.zshrc

## Install Buck2
wget https://github.com/facebook/buck2/releases/download/2025-02-01/buck2-x86_64-unknown-linux-musl.zst /home/$USERNAME/buck2-x86_64-unknown-linux-musl.zst
zstd -d /home/$USERNAME/buck2-x86_64-unknown-linux-musl.zst
mkdir -p /home/$USERNAME/.buck/bin
mv /home/$USERNAME/buck2-x86_64-unknown-linux-musl /home/$USERNAME/.buck/bin/buck2
chmod +x /home/$USERNAME/.buck/bin/buck2
chown -R $USER_UID:$USER_GID /home/$USERNAME/.buck
export PATH="/home/$USERNAME/.buck2/bin:$PATH"
