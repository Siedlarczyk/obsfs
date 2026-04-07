#!/bin/sh
# ObsFS Uninstaller
# Usage: curl -fsSL https://raw.githubusercontent.com/Siedlarczyk/obsfs/main/uninstall.sh | sh

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
RESET='\033[0m'

info() { printf "${GREEN}info${RESET} %s\n" "$1"; }
warn() { printf "${YELLOW}warn${RESET} %s\n" "$1"; }
error() { printf "${RED}error${RESET} %s\n" "$1" >&2; exit 1; }

# Check if running as root
need_sudo() {
    if [ "$(id -u)" != "0" ]; then
        if command -v sudo >/dev/null 2>&1; then
            echo "sudo"
        else
            error "This script requires root privileges. Run with sudo or as root."
        fi
    fi
}

SUDO=$(need_sudo)

main() {
    echo ""
    echo "ObsFS Uninstaller"
    echo "================="
    echo ""

    # Stop and unmount if running
    if mount | grep -q "obsfs\|/obs"; then
        info "Unmounting ObsFS..."
        $SUDO umount /obs 2>/dev/null || true
        $SUDO fusermount -u /obs 2>/dev/null || true
    fi

    # Stop systemd service if exists
    if [ -f /lib/systemd/system/obsfs.service ] || [ -f /etc/systemd/system/obsfs.service ]; then
        info "Stopping systemd service..."
        $SUDO systemctl stop obsfs 2>/dev/null || true
        $SUDO systemctl disable obsfs 2>/dev/null || true
    fi

    # Remove binary
    for bin_path in /usr/local/bin/obsfs /usr/bin/obsfs ~/.local/bin/obsfs; do
        if [ -f "$bin_path" ]; then
            info "Removing $bin_path"
            $SUDO rm -f "$bin_path"
        fi
    done

    # Remove bundled libraries
    if [ -d /usr/local/lib/obsfs ]; then
        info "Removing /usr/local/lib/obsfs"
        $SUDO rm -rf /usr/local/lib/obsfs
    fi

    # Remove config
    if [ -d /etc/obsfs ]; then
        printf "Remove config files in /etc/obsfs? [y/N] "
        read -r answer
        if [ "$answer" = "y" ] || [ "$answer" = "Y" ]; then
            info "Removing /etc/obsfs"
            $SUDO rm -rf /etc/obsfs
        else
            warn "Keeping /etc/obsfs"
        fi
    fi

    # Remove systemd service files
    for svc in /lib/systemd/system/obsfs.service /lib/systemd/system/obsfs@.service /etc/systemd/system/obsfs.service; do
        if [ -f "$svc" ]; then
            info "Removing $svc"
            $SUDO rm -f "$svc"
        fi
    done

    # Reload systemd
    if command -v systemctl >/dev/null 2>&1; then
        $SUDO systemctl daemon-reload 2>/dev/null || true
    fi

    # Remove mount point if empty
    if [ -d /obs ] && [ -z "$(ls -A /obs 2>/dev/null)" ]; then
        info "Removing empty /obs directory"
        $SUDO rmdir /obs 2>/dev/null || true
    fi

    echo ""
    echo "${GREEN}ObsFS uninstalled successfully.${RESET}"
    echo ""
}

main "$@"
