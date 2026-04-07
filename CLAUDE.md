# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

ObsFS is a FUSE daemon in Rust that exposes observability metrics as a virtual filesystem mounted at `/obs`. Users can query metrics using standard Unix tools (cat, grep, watch, tail -f) instead of dashboards or query languages.

**Important:** FUSE only works on Linux. Use OrbStack or a Linux VM for full builds and testing. The `obsfs-core` crate can be tested on macOS.

## Build Commands

```bash
# Enter dev environment (installs all dependencies)
nix develop

# Use make (task runner)
make check          # Fast compile check
make build          # Debug build
make build-release  # Release build
make test           # Run all tests
make test-core      # Test only obsfs-core (works on macOS)
make lint           # Run clippy
make fmt            # Format code
make ci             # Run all checks (fmt, lint, test)
make watch          # Auto-rebuild on file changes

# Single test
cargo test -p obsfs-core test_name

# Mount filesystem (Linux only)
make mount          # Mounts at /tmp/obs
make unmount
```

## Architecture

### Crate Structure

```
crates/
├── obsfs-core/       # Shared types and utilities (no FUSE dependency)
├── obsfs-fuse/       # FUSE filesystem implementation
├── obsfs-plugins/    # Metric providers (procfs, logs, prometheus)
└── obsfs-cli/        # Binary entry point
```

### Core Abstractions

**`MetricProvider` trait** (`obsfs-core/src/types.rs`): All collectors implement this trait. Defines `path()` (filesystem location), `collect()` (get value), and `min_interval()` (cache TTL).

**`FsNode` enum** (`obsfs-core/src/types.rs`): Represents filesystem tree nodes:
- `Directory` - contains children
- `Metric` - leaf node with a `MetricProvider`
- `Config` - writable file (e.g., `_meta/format`)

**`Registry`** (`obsfs-core/src/registry.rs`): Manages the virtual filesystem tree. Handles path-based insertion and lookup of `FsNode`s.

**`InodeTable`** (`obsfs-fuse/src/inode.rs`): Bidirectional mapping between paths and inode numbers (FUSE uses inodes, not paths).

**`ObsFs`** (`obsfs-fuse/src/fs.rs`): Implements `fuser::Filesystem` trait. Handles FUSE operations (readdir, getattr, read, write).

### Data Flow

1. User runs `cat /obs/system/cpu/usage`
2. Kernel forwards request to FUSE module
3. FUSE calls `ObsFs::read()` with inode
4. `InodeTable` resolves inode → path
5. `Registry` looks up `FsNode::Metric`
6. `MetricCache` returns cached value or calls `MetricProvider::collect()`
7. Value formatted per `OutputFormat` (plain/json) and returned

### Adding a New Collector

1. Create provider struct implementing `MetricProvider` in `obsfs-plugins/src/`
2. Register with `Registry::insert_provider()` in the collector's registration method
3. Wire up in `obsfs-cli/src/main.rs`

## Development Environment

```bash
# Using Nix (recommended)
nix develop

# Using OrbStack (for FUSE on macOS)
orb create ubuntu obsfs-dev
orb shell obsfs-dev
# Then: nix develop
```

Enable Nix flakes if needed:
```bash
echo "experimental-features = nix-command flakes" >> ~/.config/nix/nix.conf
```
