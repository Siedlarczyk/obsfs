# Systemd Service Files

## Files

| File | Description |
|------|-------------|
| `obsfs.service` | Single instance service |
| `obsfs@.service` | Template for multiple instances |

## Quick Start

```bash
# Install (from project root)
make install-service

# Or manually:
sudo cp pkg/systemd/obsfs.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now obsfs
```

## Single Instance

```bash
# Start
sudo systemctl start obsfs

# Status
sudo systemctl status obsfs

# Logs
sudo journalctl -u obsfs -f

# Stop
sudo systemctl stop obsfs
```

## Multiple Instances

Use the template unit `obsfs@.service`:

```bash
# Install template
sudo cp pkg/systemd/obsfs@.service /etc/systemd/system/
sudo systemctl daemon-reload

# Create configs for each instance
sudo cp /etc/obsfs/config.toml /etc/obsfs/app1.toml
sudo cp /etc/obsfs/config.toml /etc/obsfs/app2.toml

# Start instances (mounts at /obs/app1 and /obs/app2)
sudo systemctl start obsfs@app1
sudo systemctl start obsfs@app2

# Enable on boot
sudo systemctl enable obsfs@app1 obsfs@app2
```

## Configuration

Edit `/etc/obsfs/config.toml` (or `/etc/obsfs/<instance>.toml` for template units).

See `config/default.toml` for all options.

## Security Hardening

The service files include:

- `NoNewPrivileges=yes` - Prevent privilege escalation
- `ProtectSystem=strict` - Read-only root filesystem
- `ProtectHome=yes` - No access to home directories
- `PrivateTmp=yes` - Isolated /tmp
- `MemoryMax=256M` - Memory limit
- `CPUQuota=50%` - CPU limit

## Resource Limits

Adjust in the service file:

```ini
[Service]
MemoryMax=512M
CPUQuota=100%
```

## Troubleshooting

### Service won't start

```bash
# Check status
sudo systemctl status obsfs -l

# Check journal
sudo journalctl -u obsfs -e

# Test manually
sudo /usr/local/bin/obsfs mount /obs --allow-other
```

### Permission denied on /obs

```bash
# Ensure mount point is writable
sudo mkdir -p /obs
sudo chmod 755 /obs

# Ensure fuse.conf allows other users
echo "user_allow_other" | sudo tee -a /etc/fuse.conf
```

### Can't unmount (device busy)

```bash
# Lazy unmount
sudo fusermount -uz /obs

# Or force
sudo umount -l /obs
```
