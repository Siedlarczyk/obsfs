#!/bin/sh
# ObsFS Installer
# Usage: curl -fsSL https://raw.githubusercontent.com/Siedlarczyk/obsfs/main/install.sh | sh
#
# Environment variables:
#   OBSFS_VERSION  - Version to install (default: latest)
#   OBSFS_DIR      - Installation directory (default: /usr/local/bin or ~/.local/bin)
#   OBSFS_NO_SUDO  - Set to skip sudo (installs to ~/.local/bin)

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
DIM='\033[2m'
RESET='\033[0m'

# Config
GITHUB_REPO="Siedlarczyk/obsfs"
BINARY_NAME="obsfs"

# -----------------------------------------------------------------------------
# Helpers
# -----------------------------------------------------------------------------

info() {
    printf "${CYAN}info${RESET} %s\n" "$1"
}

success() {
    printf "${GREEN}done${RESET} %s\n" "$1"
}

warn() {
    printf "${YELLOW}warn${RESET} %s\n" "$1"
}

error() {
    printf "${RED}error${RESET} %s\n" "$1" >&2
    exit 1
}

# -----------------------------------------------------------------------------
# Detection
# -----------------------------------------------------------------------------

detect_os() {
    case "$(uname -s)" in
        Linux*)  echo "linux" ;;
        Darwin*) echo "macos" ;;
        *)       error "Unsupported OS: $(uname -s)" ;;
    esac
}

detect_arch() {
    case "$(uname -m)" in
        x86_64|amd64)  echo "x86_64" ;;
        aarch64|arm64) echo "aarch64" ;;
        *)             error "Unsupported architecture: $(uname -m)" ;;
    esac
}

detect_install_dir() {
    if [ -n "$OBSFS_DIR" ]; then
        echo "$OBSFS_DIR"
    elif [ -n "$OBSFS_NO_SUDO" ] || [ "$(id -u)" != "0" ] && ! command -v sudo >/dev/null 2>&1; then
        mkdir -p "$HOME/.local/bin"
        echo "$HOME/.local/bin"
    else
        echo "/usr/local/bin"
    fi
}

has_command() {
    command -v "$1" >/dev/null 2>&1
}

# -----------------------------------------------------------------------------
# Installation
# -----------------------------------------------------------------------------

get_latest_version() {
    if has_command curl; then
        curl -fsSL "https://api.github.com/repos/${GITHUB_REPO}/releases/latest" | grep '"tag_name"' | sed -E 's/.*"v?([^"]+)".*/\1/'
    elif has_command wget; then
        wget -qO- "https://api.github.com/repos/${GITHUB_REPO}/releases/latest" | grep '"tag_name"' | sed -E 's/.*"v?([^"]+)".*/\1/'
    else
        error "curl or wget is required"
    fi
}

download() {
    url="$1"
    output="$2"

    if has_command curl; then
        curl -fsSL "$url" -o "$output"
    elif has_command wget; then
        wget -q "$url" -O "$output"
    else
        error "curl or wget is required"
    fi
}

detect_libc() {
    # Check if using musl (Alpine, Void, etc.)
    if ldd --version 2>&1 | grep -qi musl; then
        echo "musl"
    elif [ -f /etc/alpine-release ]; then
        echo "musl"
    else
        echo "glibc"
    fi
}

install_binary() {
    os="$1"
    arch="$2"
    version="$3"
    install_dir="$4"

    # Map arch names
    case "$arch" in
        x86_64)  arch_name="x86_64" ;;
        aarch64) arch_name="aarch64" ;;
    esac

    # Detect libc type
    libc=$(detect_libc)
    if [ "$libc" = "musl" ]; then
        suffix="-musl"
        info "Detected musl libc (Alpine/Void)"
    else
        suffix=""
    fi

    # Build download URL
    archive_name="obsfs-v${version}-${arch_name}-${os}${suffix}.tar.gz"
    download_url="https://github.com/${GITHUB_REPO}/releases/download/v${version}/${archive_name}"

    info "Downloading ObsFS v${version} for ${os}/${arch} (${libc})..."

    # Create temp directory
    tmp_dir=$(mktemp -d)
    trap 'rm -rf "$tmp_dir"' EXIT

    # Download and extract
    download "$download_url" "$tmp_dir/obsfs.tar.gz" || {
        error "Failed to download from ${download_url}"
    }

    tar -xzf "$tmp_dir/obsfs.tar.gz" -C "$tmp_dir" || {
        error "Failed to extract archive"
    }

    # Install binary and libraries
    if [ "$install_dir" = "/usr/local/bin" ] && [ "$(id -u)" != "0" ]; then
        info "Installing to ${install_dir} (requires sudo)..."
        sudo install -m 755 "$tmp_dir/obsfs" "$install_dir/obsfs"
        # Install bundled libraries if present (glibc builds only)
        if [ -d "$tmp_dir/lib" ]; then
            sudo mkdir -p "$install_dir/../lib/obsfs"
            sudo cp -r "$tmp_dir/lib/"* "$install_dir/../lib/obsfs/"
            # Configure dynamic linker to find bundled libraries
            # (RPATH is ignored by sudo for security, so we use ldconfig)
            echo "/usr/local/lib/obsfs" | sudo tee /etc/ld.so.conf.d/obsfs.conf >/dev/null
            sudo ldconfig
        fi
    else
        info "Installing to ${install_dir}..."
        install -m 755 "$tmp_dir/obsfs" "$install_dir/obsfs"
        if [ -d "$tmp_dir/lib" ]; then
            mkdir -p "$install_dir/../lib/obsfs"
            cp -r "$tmp_dir/lib/"* "$install_dir/../lib/obsfs/"
            patchelf --set-rpath "$install_dir/../lib/obsfs" "$install_dir/obsfs" 2>/dev/null || true
        fi
    fi

    success "Installed to ${install_dir}/obsfs"
}

install_from_source() {
    info "No pre-built binary available. Building from source..."

    if ! has_command cargo; then
        error "Rust toolchain required. Install from https://rustup.rs"
    fi

    if ! has_command git; then
        error "git is required to build from source"
    fi

    tmp_dir=$(mktemp -d)
    trap 'rm -rf "$tmp_dir"' EXIT

    info "Cloning repository..."
    git clone --depth 1 "https://github.com/${GITHUB_REPO}.git" "$tmp_dir/obsfs"

    info "Building release binary (this may take a few minutes)..."
    cd "$tmp_dir/obsfs"
    cargo build --release

    install_dir=$(detect_install_dir)

    if [ "$install_dir" = "/usr/local/bin" ] && [ "$(id -u)" != "0" ]; then
        sudo install -m 755 "target/release/obsfs" "$install_dir/obsfs"
    else
        install -m 755 "target/release/obsfs" "$install_dir/obsfs"
    fi

    success "Installed to ${install_dir}/obsfs"
}

check_fuse() {
    os="$1"

    if [ "$os" = "linux" ]; then
        if [ ! -e /dev/fuse ]; then
            warn "FUSE not found. Install with:"
            echo "  Ubuntu/Debian: sudo apt install fuse3"
            echo "  Fedora:        sudo dnf install fuse3"
            echo "  Arch:          sudo pacman -S fuse3"
        fi
    elif [ "$os" = "macos" ]; then
        warn "macOS requires macFUSE or FUSE-T:"
        echo "  brew install macfuse"
        echo "  # or"
        echo "  brew install fuse-t"
    fi
}

print_banner() {
    printf "\n"
    printf "${CYAN}   ObsFS${RESET}\n"
    printf "${CYAN}   ├──┬──●${RESET}\n"
    printf "${CYAN}   │  └──●${RESET}\n"
    printf "${CYAN}   ├──●${RESET}\n"
    printf "${CYAN}   └──┬──●${RESET}\n"
    printf "${CYAN}      └──●${RESET}\n"
    printf "\n"
}

print_success() {
    install_dir="$1"

    printf "\n"
    printf "${GREEN}${BOLD}ObsFS installed successfully!${RESET}\n"
    printf "\n"
    printf "${DIM}Quick start:${RESET}\n"
    printf "  sudo obsfs mount /obs\n"
    printf "  cat /obs/system/cpu/usage\n"
    printf "  cat /obs/health\n"
    printf "\n"

    # Check if in PATH
    case ":$PATH:" in
        *":$install_dir:"*) ;;
        *)
            printf "${YELLOW}Note:${RESET} Add ${install_dir} to your PATH:\n"
            printf "  export PATH=\"\$PATH:${install_dir}\"\n"
            printf "\n"
            ;;
    esac

    printf "${DIM}Documentation: https://github.com/${GITHUB_REPO}${RESET}\n"
    printf "\n"
}

# -----------------------------------------------------------------------------
# Main
# -----------------------------------------------------------------------------

main() {
    print_banner

    os=$(detect_os)
    arch=$(detect_arch)
    version="${OBSFS_VERSION:-$(get_latest_version)}"
    install_dir=$(detect_install_dir)

    info "Detected: ${os}/${arch}"

    # Check for macOS (not fully supported yet)
    if [ "$os" = "macos" ]; then
        warn "macOS support is experimental"
        check_fuse "$os"
        install_from_source
    else
        # Try binary first, fall back to source
        install_binary "$os" "$arch" "$version" "$install_dir" 2>/dev/null || {
            warn "Pre-built binary not found, building from source..."
            install_from_source
        }
        check_fuse "$os"
    fi

    print_success "$install_dir"
}

main "$@"
