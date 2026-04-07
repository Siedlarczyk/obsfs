# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-04-06

### Added

- **Core Framework**
  - FUSE filesystem implementation with inode management
  - Plugin system with unified `Plugin` trait
  - Registry for filesystem tree management
  - Support for static (`MetricProvider`) and dynamic (`DynamicHandler`) paths
  - Configurable output formats (plain text, JSON)

- **Plugins**
  - `procsys` - System metrics from /proc and /sys (CPU, memory, disk, network, filesystem)
  - `health` - Aggregated system health check with status and alerts
  - `proc_info` - Per-process information at `/obs/proc/[pid]`
  - `connections` - Network connections (TCP/UDP, listening, established)
  - `services` - Systemd service status at `/obs/services/[name]`
  - `sensors` - Hardware sensors (temperature, fans)
  - `users` - User sessions and per-user information
  - `docker` - Docker container metrics at `/obs/docker/[container]`

- **CLI**
  - `obsfs mount` - Mount the filesystem
  - `obsfs unmount` - Unmount the filesystem
  - `obsfs status` - Show mount status
  - `obsfs version` - Show version information

- **Documentation**
  - Architecture documentation
  - Plugin development guide
  - Per-plugin documentation with examples

### Technical Details

- Written in Rust with async support
- Uses `fuser` crate for FUSE implementation
- Supports Linux (full) and macOS (core libraries only)
- Thread-safe design with `Send + Sync` traits

[Unreleased]: https://github.com/Siedlarczyk/obsfs/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Siedlarczyk/obsfs/releases/tag/v0.1.0
