#!/bin/bash
# Build .deb package for ObsFS
# Usage: ./build-deb.sh <version> <arch> <binary-path>

set -e

VERSION="${1:-0.1.0}"
VERSION="${VERSION#v}"  # Remove 'v' prefix if present
ARCH="${2:-amd64}"  # amd64 or arm64
BINARY="${3:-../../target/release/obsfs}"

PKG_NAME="obsfs"
PKG_DIR="${PKG_NAME}_${VERSION}_${ARCH}"

echo "Building ${PKG_DIR}.deb..."

# Clean and create directory structure
rm -rf "$PKG_DIR"
mkdir -p "$PKG_DIR/DEBIAN"
mkdir -p "$PKG_DIR/usr/bin"
mkdir -p "$PKG_DIR/etc/obsfs"
mkdir -p "$PKG_DIR/lib/systemd/system"
mkdir -p "$PKG_DIR/usr/share/doc/obsfs"

# Copy binary
cp "$BINARY" "$PKG_DIR/usr/bin/obsfs"
chmod 755 "$PKG_DIR/usr/bin/obsfs"

# Copy config
cp ../../config/default.toml "$PKG_DIR/etc/obsfs/config.toml"

# Copy systemd service
cp ../systemd/obsfs.service "$PKG_DIR/lib/systemd/system/"

# Copy docs
cp ../../README.md "$PKG_DIR/usr/share/doc/obsfs/"
cp ../../LICENSE "$PKG_DIR/usr/share/doc/obsfs/" 2>/dev/null || echo "MIT License" > "$PKG_DIR/usr/share/doc/obsfs/LICENSE"

# Calculate installed size (in KB)
INSTALLED_SIZE=$(du -sk "$PKG_DIR" | cut -f1)

# Create control file
cat > "$PKG_DIR/DEBIAN/control" << EOF
Package: ${PKG_NAME}
Version: ${VERSION}
Section: admin
Priority: optional
Architecture: ${ARCH}
Installed-Size: ${INSTALLED_SIZE}
Depends: fuse3
Maintainer: ObsFS Contributors <obsfs@users.noreply.github.com>
Homepage: https://github.com/Siedlarczyk/obsfs
Description: Observability Filesystem
 ObsFS mounts a virtual filesystem where each file returns an
 observability metric. Use cat, grep, watch, and other Unix
 tools to query system metrics.
EOF

# Create postinst script
cat > "$PKG_DIR/DEBIAN/postinst" << 'EOF'
#!/bin/bash
set -e

# Reload systemd
if [ -d /run/systemd/system ]; then
    systemctl daemon-reload || true
fi

echo ""
echo "ObsFS installed successfully!"
echo ""
echo "Quick start:"
echo "  sudo obsfs mount /obs"
echo "  cat /obs/system/cpu/usage"
echo ""
echo "To enable as a service:"
echo "  sudo systemctl enable --now obsfs"
echo ""
EOF
chmod 755 "$PKG_DIR/DEBIAN/postinst"

# Create prerm script
cat > "$PKG_DIR/DEBIAN/prerm" << 'EOF'
#!/bin/bash
set -e

# Stop service if running
if [ -d /run/systemd/system ]; then
    systemctl stop obsfs || true
    systemctl disable obsfs || true
fi
EOF
chmod 755 "$PKG_DIR/DEBIAN/prerm"

# Create conffiles (mark config as conffile)
cat > "$PKG_DIR/DEBIAN/conffiles" << EOF
/etc/obsfs/config.toml
EOF

# Build the package
dpkg-deb --build --root-owner-group "$PKG_DIR"

echo "Created ${PKG_DIR}.deb"

# Cleanup
rm -rf "$PKG_DIR"
