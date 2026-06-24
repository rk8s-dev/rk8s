#!/bin/bash
set -euo pipefail

# ----------------------------------------
# Check and install Rust
# ----------------------------------------
check_rust() {
    if command -v rustc &> /dev/null && rustc -V &> /dev/null; then
        echo "✅ Rust is already installed: $(rustc -V)"
    else
        echo "⚠️  Rust is not installed or 'rustc' is not available. Installing Rust..."

        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        . "$HOME/.cargo/env"

        echo "🔧 Rust installation completed."
fi
}

# ----------------------------------------
# Check and install Buck2
# ----------------------------------------
check_buck2() {
    if command -v buck2 &> /dev/null && buck2 --version &> /dev/null; then
        echo "✅ Buck2 is already installed: $(buck2 --version | head -n1)"
    else
        echo "⚠️  Buck2 is not installed or 'buck2' command is not available. Installing Buck2..."

        export ARCH="$(uname -m)"
        curl "https://github.com/facebook/buck2/releases/download/latest/buck2-${ARCH}-unknown-linux-gnu.zst" --output /tmp/buck2-${ARCH}-unknown-linux-gnu.zst --location
        zstd -d /tmp/buck2-${ARCH}-unknown-linux-gnu.zst -o $HOME/.cargo/bin/buck2 -f
        chmod +x $HOME/.cargo/bin/buck2

        echo "🔧 Buck2 installation completed."
    fi
}

# ----------------------------------------
# Check and install cargo-buckal
# ----------------------------------------
check_buckal() {
    if command -v cargo-buckal &> /dev/null; then
        echo "✅ cargo-buckal is already installed: $(cargo-buckal -V)"
    else
        echo "⚠️  cargo-buckal is not installed. Installing..."
        cargo install --git https://github.com/buck2hub/cargo-buckal.git
        echo "🔧 cargo-buckal installation completed."
    fi
}

# ----------------------------------------
# Install system dependencies
# ----------------------------------------
install_system_deps() {
    echo "🔍 Detecting Linux distribution..."

    DISTRO=""

    if [ -f /etc/os-release ]; then
        . /etc/os-release
        DISTRO_ID="${ID}"
        DISTRO_VERSION="${VERSION_ID:-}"
    elif command -v lsb_release &> /dev/null; then
        DISTRO_ID="$(lsb_release -si | tr '[:upper:]' '[:lower:]')"
    else
        echo "❌ Cannot detect Linux distribution."
        exit 1
    fi

    case "${DISTRO_ID}" in
        ubuntu|debian)
            echo "🧾 Detected: ${DISTRO_ID^} (version: ${DISTRO_VERSION})"
            echo "📦 Installing system dependencies via APT..."

            sudo apt-get update
            sudo apt-get install -y \
                build-essential \
                clang \
                lld \
                pkg-config \
                protobuf-compiler \
                seccomp \
                libseccomp-dev \
                libelf-dev \
                libpython3-dev \
                openssl \
                libssl-dev \
                zstd
            ;;

        fedora)
            echo "🧾 Detected: Fedora (version: ${DISTRO_VERSION})"
            echo "📦 Installing system dependencies via DNF..."


            sudo dnf group install -y development-tools
            sudo dnf install -y \
                clang \
                lld \
                pkgconf \
                protobuf-devel \
                protobuf-compiler \
                libseccomp \
                libseccomp-devel \
                elfutils-libelf-devel \
                python3-devel \
                openssl \
                openssl-devel \
                zstd
            ;;

        arch)
            echo "🧾 Detected: Arch Linux"
            echo "📦 Installing system dependencies via Pacman..."

            
            sudo pacman -Sy --noconfirm
            sudo pacman -S --noconfirm \
                base-devel \
                clang \
                lld \
                pkgconf \
                protobuf \
                protobuf-c \
                libseccomp \
                libelf \
                python \
                python-setuptools \
                openssl \
                zstd
            ;;
        
        *)
            echo "⚠️  Unknown or unsupported distribution: ${DISTRO_ID}"
            echo "💡 You may need to manually install dependencies."
            exit 1
            ;;
    esac

    echo "✅ System dependencies installation complete."
}

# ----------------------------------------
# Execute workflow
# ----------------------------------------
echo "🚀 Starting setup script..."

install_system_deps
check_rust
check_buck2
check_buckal

echo "🎉 All setup completed successfully!"