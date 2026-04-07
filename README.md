<p align="center">
  <pre align="center">
   ┌─────────────────┐
   │     ObsFS       │
   │     ├──┬──●     │
   │     │  └──●     │
   │     ├──●        │
   │     └──┬──●     │
   │        └──●     │
   └─────────────────┘
  </pre>
  <strong>Observe everything as files</strong>
</p>

<p align="center">
  <a href="https://github.com/Siedlarczyk/obsfs/actions/workflows/ci.yml">
    <img src="https://github.com/Siedlarczyk/obsfs/actions/workflows/ci.yml/badge.svg" alt="CI">
  </a>
  <a href="https://opensource.org/licenses/MIT">
    <img src="https://img.shields.io/badge/License-MIT-yellow.svg" alt="License: MIT">
  </a>
  <a href="https://www.rust-lang.org/">
    <img src="https://img.shields.io/badge/rust-1.70%2B-orange.svg" alt="Rust 1.70+">
  </a>
</p>

<p align="center">
  A FUSE filesystem that exposes system metrics as plain text files.<br>
  Query metrics with <code>cat</code>, <code>grep</code>, <code>watch</code> — no dashboards, no query languages.
</p>

---

## Demo

```bash
$ cat /obs/system/cpu/usage
23.45

$ cat /obs/system/memory/available
8589934592

$ watch -n 1 cat /obs/health
status: healthy
cpu: 23.4% (ok)
memory: 45.2% (ok)
disk: 67.8% (ok)
```

## Features

| Feature | Description |
|---------|-------------|
| **Zero dependencies** | No agents, databases, or external services |
| **Unix philosophy** | Everything is a file — compose with pipes |
| **Real-time** | Metrics update on every read |
| **Scriptable** | Works with bash, Python, or any language |
| **Lightweight** | Single binary, minimal resource usage |

## Installation

### Quick install (Linux)

```bash
curl -fsSL https://raw.githubusercontent.com/Siedlarczyk/obsfs/main/install.sh | sh
```

### From source

```bash
git clone https://github.com/Siedlarczyk/obsfs
cd obsfs
make build-release
sudo make install
```

### With Nix

```bash
nix build
# or enter dev shell
nix develop
```

### Docker

```bash
docker run --privileged ghcr.io/Siedlarczyk/obsfs:latest
```

## Quick Start

```bash
# Mount the filesystem
sudo obsfs mount /obs

# Read metrics
cat /obs/system/cpu/usage      # CPU percentage
cat /obs/system/memory/total   # Total RAM in bytes
cat /obs/health                # System health summary

# Watch in real-time
watch -n 1 cat /obs/system/cpu/load_1m

# Unmount
sudo obsfs unmount /obs
```

## Filesystem Layout

```
/obs/
├── health                    # Overall system health
├── system/
│   ├── cpu/
│   │   ├── usage             # CPU usage %
│   │   └── load_1m           # 1-minute load average
│   ├── memory/
│   │   ├── total             # Total RAM (bytes)
│   │   └── available         # Available RAM (bytes)
│   ├── disk/<device>/        # Per-disk I/O stats
│   ├── fs/<mount>/           # Per-filesystem usage
│   └── net/<interface>/      # Per-interface traffic
├── proc/<pid>/               # Process information
├── connections/              # Network connections
├── services/<name>/          # Systemd service status
├── docker/<container>/       # Docker container stats
└── _meta/format              # Output format (plain/json)
```

<details>
<summary><strong>Full filesystem structure</strong></summary>

```
/obs/
├── health
├── system/
│   ├── cpu/
│   │   ├── usage
│   │   ├── load_1m
│   │   ├── load_5m
│   │   └── load_15m
│   ├── memory/
│   │   ├── total
│   │   ├── available
│   │   ├── free
│   │   ├── swap_total
│   │   └── swap_used
│   ├── disk/<device>/
│   │   ├── read_bytes
│   │   ├── write_bytes
│   │   ├── read_iops
│   │   └── write_iops
│   ├── fs/<mount>/
│   │   ├── total
│   │   ├── used
│   │   ├── available
│   │   └── percent_used
│   ├── net/<interface>/
│   │   ├── rx_bytes
│   │   ├── tx_bytes
│   │   ├── rx_packets
│   │   └── tx_packets
│   └── uptime
├── proc/<pid>/
├── connections/
│   ├── tcp
│   ├── udp
│   ├── listening
│   ├── established
│   └── summary
├── services/<name>/
├── sensors/
│   ├── temperatures
│   ├── fans
│   └── summary
├── users/
│   ├── active
│   ├── summary
│   └── <username>/
├── docker/<container>/
│   ├── status
│   ├── stats
│   └── info
└── _meta/
    └── format
```

</details>

## Examples

### Monitoring Scripts

```bash
# Alert on high CPU
cpu=$(cat /obs/system/cpu/usage)
if (( $(echo "$cpu > 80" | bc -l) )); then
    echo "High CPU: ${cpu}%"
fi

# Log memory usage every minute
while true; do
    echo "$(date): $(cat /obs/system/memory/available)" >> memory.log
    sleep 60
done
```

### JSON Output

```bash
# Enable JSON mode
echo "json" > /obs/_meta/format

# Get structured data
cat /obs/system/cpu/usage | jq '.value'
```

### Integration

```bash
# Send to monitoring system
curl -X POST https://metrics.example.com \
    -d "cpu=$(cat /obs/system/cpu/usage)"

# Prometheus-style output
echo "cpu_usage $(cat /obs/system/cpu/usage)"
```

## Deployment

| Method | Command | Notes |
|--------|---------|-------|
| **Systemd** | `make install-service` | Recommended for servers |
| **Docker** | `make docker-run` | Requires `--privileged` |
| **Docker Compose** | `docker compose up -d` | See `docker-compose.yml` |
| **Nix** | `nix run` | Reproducible builds |

See [docs/deployment.md](docs/deployment.md) for detailed instructions.

## Configuration

```toml
# /etc/obsfs/config.toml

[mount]
path = "/obs"
allow_other = true

[format]
default = "plain"  # or "json"

[logging]
level = "info"
format = "compact"  # or "json", "pretty"
```

## Requirements

- **Linux** with FUSE support (kernel 2.6.14+)
- **FUSE 3** libraries (`libfuse3-dev` on Debian/Ubuntu)
- For Docker: `--privileged` or `--cap-add SYS_ADMIN --device /dev/fuse`

> **Note**: macOS is not supported (no FUSE). Use Docker or a Linux VM.

## Development

```bash
# Enter dev environment
nix develop

# Build and test
make check          # Fast compile check
make test           # Run all tests
make test-core      # Test core crate (works on macOS)
make lint           # Run clippy

# Mount locally
make mount          # Mounts at /tmp/obs
make unmount
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for contribution guidelines.

## Project Structure

```
crates/
├── obsfs-core/       # Core types and registry
├── obsfs-fuse/       # FUSE implementation
├── obsfs-plugins/    # Metric plugins
└── obsfs-cli/        # CLI binary

pkg/
├── systemd/          # Systemd service files
└── docker/           # Dockerfiles

docs/                 # Documentation
```

## Roadmap

- [x] System metrics (CPU, memory, disk, network)
- [x] Process information
- [x] Network connections
- [x] Systemd services
- [x] Hardware sensors
- [x] Docker containers
- [ ] Log file streaming
- [ ] Prometheus scraping
- [ ] GPU metrics (NVIDIA)

## License

[MIT](LICENSE)

## Links

- [Documentation](docs/)
- [Deployment Guide](docs/deployment.md)
- [Contributing](CONTRIBUTING.md)
- [Security Policy](SECURITY.md)
- [Changelog](CHANGELOG.md)
