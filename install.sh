#!/bin/sh
# rk8s installer - A robust installation script for rk8s components
# Usage: curl -sSf https://download.rk8s.dev/install.sh | sh
#        or with options: curl -sSf https://download.rk8s.dev/install.sh | sh -s -- --all

set -e

# Configuration
BASE_URL="${RK8S_BASE_URL:-https://download.rk8s.dev/rk8s/releases}"
INSTALL_DIR="${RK8S_INSTALL_DIR:-/usr/local/bin}"
COMPONENTS="rkforge distribution rks rkl slayerfs"
DEFAULT_COMPONENT="rkforge"
DEFAULT_VERSION="v0.1.2"
INSTALL_SANDBOX_RUNTIME="${RK8S_INSTALL_SANDBOX_RUNTIME:-1}"

# Color codes for output
if [ -t 1 ]; then
    RED='\033[0;31m'
    GREEN='\033[0;32m'
    YELLOW='\033[1;33m'
    BLUE='\033[0;34m'
    NC='\033[0m' # No Color
else
    RED=''
    GREEN=''
    YELLOW=''
    BLUE=''
    NC=''
fi

# Logging functions
info() {
    printf "${BLUE}info:${NC} %s\n" "$1"
}

success() {
    printf "${GREEN}success:${NC} %s\n" "$1"
}

warn() {
    printf "${YELLOW}warning:${NC} %s\n" "$1" >&2
}

error() {
    printf "${RED}error:${NC} %s\n" "$1" >&2
    exit 1
}

# Usage information
usage() {
    cat <<EOF
rk8s installer

USAGE:
    install.sh [OPTIONS]

OPTIONS:
    -c, --components <LIST>    Comma-separated list of components to install
                               Available: rkforge, distribution, rks, rkl, slayerfs
    -a, --all                  Install all components
    -v, --version <VERSION>    Specify version (default: latest)
    -d, --dir <PATH>           Installation directory (default: /usr/local/bin)
    -h, --help                 Show this help message

EXAMPLES:
    # Install default component (rkforge)
    curl -sSf https://download.rk8s.dev/install.sh | sh

    # Install specific components
    curl -sSf https://download.rk8s.dev/install.sh | sh -s -- -c rks,rkl

    # Install all components
    curl -sSf https://download.rk8s.dev/install.sh | sh -s -- --all

    # Install to custom directory
    curl -sSf https://download.rk8s.dev/install.sh | sh -s -- -d ~/.rk8s/bin

ENVIRONMENT VARIABLES:
    RK8S_VERSION               Override version detection
    RK8S_INSTALL_DIR           Override installation directory
    RK8S_BASE_URL              Override download base URL
    RK8S_INSTALL_SANDBOX_RUNTIME
                               Set to 0 to skip rkforge sandbox runtime installation

EOF
    exit 0
}

# Parse command line arguments
parse_args() {
    SELECTED_COMPONENTS=""
    VERSION="${RK8S_VERSION:-}"

    while [ $# -gt 0 ]; do
        case "$1" in
            -h|--help)
                usage
                ;;
            -c|--components)
                SELECTED_COMPONENTS="$2"
                shift 2
                ;;
            -a|--all)
                SELECTED_COMPONENTS="$COMPONENTS"
                shift
                ;;
            -v|--version)
                VERSION="$2"
                shift 2
                ;;
            -d|--dir)
                INSTALL_DIR="$2"
                shift 2
                ;;
            *)
                error "Unknown option: $1. Use --help for usage information."
                ;;
        esac
    done

    # Default to rkforge if no components specified
    if [ -z "$SELECTED_COMPONENTS" ]; then
        SELECTED_COMPONENTS="$DEFAULT_COMPONENT"
    fi
}

# Detect OS
detect_os() {
    OS="$(uname -s)"
    case "$OS" in
        Linux)
            OS="linux"
            ;;
        Darwin)
            OS="darwin"
            ;;
        *)
            error "Unsupported operating system: $OS"
            ;;
    esac
    echo "$OS"
}

# Detect architecture
detect_arch() {
    ARCH="$(uname -m)"
    case "$ARCH" in
        x86_64|amd64)
            ARCH="amd64"
            ;;
        aarch64|arm64)
            ARCH="arm64"
            ;;
        *)
            error "Unsupported architecture: $ARCH"
            ;;
    esac
    echo "$ARCH"
}

detect_rust_target() {
    case "${OS}/${ARCH}" in
        linux/amd64)
            echo "x86_64-unknown-linux-gnu"
            ;;
        linux/arm64)
            echo "aarch64-unknown-linux-gnu"
            ;;
        darwin/arm64)
            echo "aarch64-apple-darwin"
            ;;
        *)
            return 1
            ;;
    esac
}

# Check for required tools
check_dependencies() {
    if command -v curl >/dev/null 2>&1; then
        DOWNLOADER="curl"
    elif command -v wget >/dev/null 2>&1; then
        DOWNLOADER="wget"
    else
        error "Neither curl nor wget found. Please install one of them."
    fi
}

# Download file
download_file() {
    local url="$1"
    local output="$2"

    if [ "$DOWNLOADER" = "curl" ]; then
        curl -fsSL "$url" -o "$output" || return 1
    else
        wget -q "$url" -O "$output" || return 1
    fi
    return 0
}

ensure_dir() {
    local dir="$1"
    if [ -d "$dir" ]; then
        return 0
    fi
    if mkdir -p "$dir" 2>/dev/null; then
        return 0
    fi
    if command -v sudo >/dev/null 2>&1; then
        sudo mkdir -p "$dir"
        return 0
    fi
    error "Cannot create directory $dir. Try: export RK8S_INSTALL_DIR=~/.rk8s/bin"
}

copy_tree() {
    local src="$1"
    local dest="$2"
    ensure_dir "$dest"
    if [ -w "$dest" ]; then
        cp -a "$src"/. "$dest"/
    else
        if command -v sudo >/dev/null 2>&1; then
            sudo cp -a "$src"/. "$dest"/
        else
            error "No write permission to $dest and sudo not available."
        fi
    fi
}

install_rkforge_sandbox_runtime() {
    if [ "$INSTALL_SANDBOX_RUNTIME" = "0" ]; then
        warn "Skipping rkforge sandbox runtime installation (RK8S_INSTALL_SANDBOX_RUNTIME=0)"
        return 0
    fi

    if [ "$OS" != "linux" ]; then
        warn "Sandbox runtime installation is currently only enabled on Linux, skipping..."
        return 0
    fi

    local rust_target
    rust_target="$(detect_rust_target)" || {
        warn "No sandbox runtime target mapping for ${OS}/${ARCH}, skipping..."
        return 0
    }

    local runtime_name="rkforge-sandbox-runtime-${VERSION}-${rust_target}.tar.gz"
    local runtime_url="${BASE_URL}/${VERSION}/${runtime_name}"
    local runtime_tar="${TEMP_DIR}/${runtime_name}"
    local runtime_extract="${TEMP_DIR}/sandbox-runtime"
    local runtime_root
    runtime_root="$(dirname "$INSTALL_DIR")"
    local runtime_lib_dir="${runtime_root}/lib"

    info "Downloading rkforge sandbox runtime from $runtime_url..."
    if ! download_file "$runtime_url" "$runtime_tar"; then
        error "Failed to download rkforge sandbox runtime. Set RK8S_INSTALL_SANDBOX_RUNTIME=0 to skip sandbox support."
    fi

    ensure_dir "$runtime_extract"
    tar -xzf "$runtime_tar" -C "$runtime_extract" || error "Failed to extract sandbox runtime archive"

    if [ ! -d "$runtime_extract/runtime/bin" ] || [ ! -d "$runtime_extract/runtime/lib" ]; then
        error "Sandbox runtime archive has unexpected layout"
    fi

    copy_tree "$runtime_extract/runtime/bin" "$INSTALL_DIR"
    copy_tree "$runtime_extract/runtime/lib" "$runtime_lib_dir"
    success "Installed rkforge sandbox runtime successfully"
}

# Fetch latest version from GitHub API
fetch_latest_version() {
    local api_url="https://api.github.com/repos/rk8s-dev/rk8s/releases/latest"
    local version=""

    if [ "$DOWNLOADER" = "curl" ]; then
        version=$(curl -fsSL "$api_url" | grep '"tag_name":' | head -n1 | sed -E 's/.*"tag_name": "([^"]+)".*/\1/' 2>/dev/null || true)
    else
        version=$(wget -qO- "$api_url" | grep '"tag_name":' | head -n1 | sed -E 's/.*"tag_name": "([^"]+)".*/\1/' 2>/dev/null || true)
    fi

    if [ -z "$version" ]; then
        version="$DEFAULT_VERSION"
    fi

    echo "$version"
}

# Pre-flight checks
preflight_checks() {
    info "Running pre-flight checks..."

    # Check GLIBC version on Linux
    if [ "$OS" = "linux" ]; then
        if command -v ldd >/dev/null 2>&1; then
            GLIBC_VERSION=$(ldd --version 2>&1 | head -n1 | grep -oE '[0-9]+\.[0-9]+' | head -n1)
            info "Detected GLIBC version: $GLIBC_VERSION"

            # rk8s requires GLIBC 2.31+ (Ubuntu 20.04+)
            GLIBC_MAJOR=$(echo "$GLIBC_VERSION" | cut -d. -f1)
            GLIBC_MINOR=$(echo "$GLIBC_VERSION" | cut -d. -f2)

            if [ "$GLIBC_MAJOR" -lt 2 ] || { [ "$GLIBC_MAJOR" -eq 2 ] && [ "$GLIBC_MINOR" -lt 31 ]; }; then
                warn "GLIBC version $GLIBC_VERSION detected. rk8s may require GLIBC 2.31 or higher."
            fi
        fi
    fi

    # Check available disk space (require at least 100MB)
    if command -v df >/dev/null 2>&1; then
        AVAILABLE_KB=$(df -k "$(dirname "$INSTALL_DIR")" | tail -1 | awk '{print $4}')
        if [ "$AVAILABLE_KB" -lt 102400 ]; then
            warn "Low disk space detected. At least 100MB recommended."
        fi
    fi
}

# Validate component availability for OS/arch
validate_component() {
    local component="$1"

    # macOS (darwin) only supports distribution component
    if [ "$OS" = "darwin" ]; then
        case "$component" in
            distribution)
                return 0
                ;;
            *)
                return 1
                ;;
        esac
    fi

    return 0
}

# Install a single component
install_component() {
    local component="$1"
    local binary_name="${component}-${OS}-${ARCH}"
    local download_url="${BASE_URL}/${VERSION}/${binary_name}"
    local temp_file="${TEMP_DIR}/${binary_name}"

    # Validate component for this platform
    if ! validate_component "$component"; then
        warn "Component '$component' is not available for ${OS}/${ARCH}, skipping..."
        return 0
    fi

    info "Downloading $component from $download_url..."

    if ! download_file "$download_url" "$temp_file"; then
        error "Failed to download $component. Please check version and component name."
    fi

    # Verify download
    if [ ! -f "$temp_file" ] || [ ! -s "$temp_file" ]; then
        error "Downloaded file is empty or missing: $temp_file"
    fi

    # Make executable
    chmod +x "$temp_file"

    # Install to target directory
    local target_path="${INSTALL_DIR}/${component}"

    info "Installing $component to $target_path..."

    # Check if we need sudo
    if [ -w "$INSTALL_DIR" ]; then
        mv "$temp_file" "$target_path"
    else
        if command -v sudo >/dev/null 2>&1; then
            sudo mv "$temp_file" "$target_path"
        else
            error "No write permission to $INSTALL_DIR and sudo not available. Try: export RK8S_INSTALL_DIR=~/.rk8s/bin"
        fi
    fi

    success "Installed $component successfully"

    if [ "$component" = "rkforge" ]; then
        install_rkforge_sandbox_runtime
    fi
}

# Main installation logic
main() {
    parse_args "$@"

    info "rk8s installer"
    info "==============="

    # Detect system
    OS=$(detect_os)
    ARCH=$(detect_arch)
    info "Detected platform: ${OS}/${ARCH}"

    # Check dependencies
    check_dependencies
    info "Using downloader: $DOWNLOADER"

    # Get version
    if [ -z "$VERSION" ]; then
        VERSION=$(fetch_latest_version)
        info "Fetched version: $VERSION"
    else
        info "Using specified version: $VERSION"
    fi

    # Pre-flight checks
    preflight_checks

    # Create temporary directory
    TEMP_DIR=$(mktemp -d)
    trap 'rm -rf "$TEMP_DIR"' EXIT

    # Create install directory if it doesn't exist
    if [ ! -d "$INSTALL_DIR" ]; then
        info "Creating installation directory: $INSTALL_DIR"
        if mkdir -p "$INSTALL_DIR" 2>/dev/null; then
            :
        else
            if command -v sudo >/dev/null 2>&1; then
                sudo mkdir -p "$INSTALL_DIR"
            else
                error "Cannot create $INSTALL_DIR. Try: export RK8S_INSTALL_DIR=~/.rk8s/bin"
            fi
        fi
    fi

    # Install each component
    info "Components to install: $SELECTED_COMPONENTS"

    for component in $(echo "$SELECTED_COMPONENTS" | tr ',' ' '); do
        install_component "$component"
    done

    # Final success message
    echo ""
    success "Installation complete!"
    info "Installed to: $INSTALL_DIR"

    # Check if install dir is in PATH
    case ":$PATH:" in
        *":$INSTALL_DIR:"*)
            ;;
        *)
            echo ""
            warn "The installation directory is not in your PATH."
            info "Add it to your PATH by running:"
            info "  export PATH=\"\$PATH:$INSTALL_DIR\""
            info ""
            info "To make it permanent, add the above line to your shell profile:"
            info "  ~/.bashrc (bash) or ~/.zshrc (zsh)"
            ;;
    esac

    echo ""
    info "Verify installation by running:"
    for component in $(echo "$SELECTED_COMPONENTS" | tr ',' ' '); do
        if validate_component "$component"; then
            info "  $component --version"
        fi
    done
}

# Run main function
main "$@"
