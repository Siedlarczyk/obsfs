# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-04-07

### Added

- **Daemon Mode**: `--daemon` / `-d` flag now properly daemonizes the process using `fork()` + `setsid()`, writes PID file to `/var/run/obsfs.pid`
- **File Logging**: `LogOutput::File` now works with `tracing-appender` for daily rolling log files
- **Plugin Status Endpoint**: New `/_meta/plugins` endpoint shows registration status of all plugins
- **InodeTable Eviction**: Added `mark_stale()` and `sweep_stale()` methods to prevent memory leaks from dynamic paths

### Changed

- **Graceful Plugin Degradation**: Plugin registration failures no longer crash the daemon; failed plugins are logged and skipped
- **CPU Measurement**: Now uses background thread with delta calculation between snapshots (was: single snapshot since boot)
- **Docker Client**: HTTP responses are now validated; non-2xx status codes return errors instead of being parsed as data
- **RwLock Handling**: Poisoned locks now return `EIO` to FUSE instead of panicking, preventing daemon crashes
- **Unified `format_bytes`**: Consolidated 5 duplicate implementations into `obsfs_core::utils::format_bytes()` with consistent IEC notation (KiB/MiB/GiB/TiB)
- **Safer FFI**: Replaced `unsafe { libc::getuid() }` with `nix::unistd::getuid()`, and `mem::zeroed()` with `MaybeUninit`

### Removed

- **`MetricValue::Stream`**: Removed dead code variant that was never implemented

### Fixed

- CPU usage now reflects actual recent activity instead of cumulative average since boot
- Docker API errors (404, 500) are now properly reported instead of causing parse failures

### Infrastructure

- `install.sh`: Added musl/Alpine detection for correct binary selection
- `uninstall.sh`: New script to cleanly remove ObsFS
- README: Added .deb installation instructions and uninstall section

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

[Unreleased]: https://github.com/Siedlarczyk/obsfs/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/Siedlarczyk/obsfs/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/Siedlarczyk/obsfs/releases/tag/v0.1.0
