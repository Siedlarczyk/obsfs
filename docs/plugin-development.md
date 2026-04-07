# ObsFS Plugin Development Guide

This guide covers how to create plugins that extend ObsFS with new metrics and dynamic handlers.

## Overview

ObsFS plugins are the primary way to extend the system with new metrics. Each plugin can provide:

1. **Static Metrics** - Fixed filesystem paths that return metric values (e.g., `/obs/health`)
2. **Dynamic Handlers** - Runtime-generated paths based on system state (e.g., `/obs/proc/[pid]`)

There are two key patterns:

### Static Provider Pattern

Implement `MetricProvider` for individual metrics that always exist at a fixed path.

```
/obs/health
/obs/system/cpu/usage
/obs/system/memory/percent
```

### Dynamic Handler Pattern

Implement `DynamicHandler` for paths where entries are discovered at runtime. The system enumerates entries when the directory is listed.

```
/obs/proc/          (list PIDs)
/obs/proc/1234      (get info for PID 1234)
/obs/proc/5678      (get info for PID 5678)
```

---

## Quick Start: Creating Your First Plugin

Here's a minimal plugin that adds a single static metric:

```rust
use std::sync::Arc;
use anyhow::Result;
use obsfs_core::{MetricProvider, MetricValue, Plugin, Registry};

// Step 1: Implement MetricProvider for your metric
pub struct UptimeProvider;

impl MetricProvider for UptimeProvider {
    fn path(&self) -> &str {
        "system/uptime"
    }

    fn collect(&self) -> Result<MetricValue> {
        let uptime_str = std::fs::read_to_string("/proc/uptime")?;
        let secs: u64 = uptime_str
            .split_whitespace()
            .next()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        Ok(MetricValue::Gauge(secs as f64))
    }
}

// Step 2: Implement Plugin to register the provider
pub struct UptimePlugin;

impl Plugin for UptimePlugin {
    fn name(&self) -> &str {
        "uptime"
    }

    fn description(&self) -> &str {
        "System uptime in seconds"
    }

    fn register(&self, registry: &mut Registry) -> Result<()> {
        registry.insert_provider(Arc::new(UptimeProvider))?;
        Ok(())
    }
}

// Step 3: Wire up in main.rs (see Registration section below)
```

Usage:
```bash
$ cat /obs/system/uptime
12345678.00
```

---

## The Plugin Trait

All plugins implement `obsfs_core::Plugin`:

```rust
pub trait Plugin: Send + Sync {
    /// Returns the unique name of this plugin
    fn name(&self) -> &str;

    /// Optional human-readable description
    fn description(&self) -> &str {
        ""
    }

    /// Register static metric providers
    fn register(&self, registry: &mut Registry) -> anyhow::Result<()>;

    /// Optional: return dynamic handlers for runtime-discovered paths
    fn dynamic_handlers(&self) -> Vec<Arc<dyn DynamicHandler>> {
        vec![]
    }
}
```

### Key Methods

**`name()` - Plugin Identifier**

Returns a unique name, used for logging and identification:

```rust
fn name(&self) -> &str {
    "my-plugin"  // No spaces, use kebab-case
}
```

**`description()` - Optional Documentation**

Provides a human-readable summary:

```rust
fn description(&self) -> &str {
    "Collects metrics from the awesome service"
}
```

**`register()` - Register Static Metrics**

Called once at startup. Register all static metrics here:

```rust
fn register(&self, registry: &mut Registry) -> anyhow::Result<()> {
    registry.insert_provider(Arc::new(MyMetric1))?;
    registry.insert_provider(Arc::new(MyMetric2))?;
    registry.insert_provider(Arc::new(MyMetric3))?;
    tracing::info!("Registered my-plugin");
    Ok(())
}
```

**`dynamic_handlers()` - Optional Dynamic Paths**

Return handlers for runtime-discovered paths (like `/proc/[pid]`):

```rust
fn dynamic_handlers(&self) -> Vec<Arc<dyn DynamicHandler>> {
    vec![Arc::new(MyDynamicHandler)]
}
```

---

## MetricProvider Trait: Static Metrics

Implement `MetricProvider` for each individual metric:

```rust
pub trait MetricProvider: Send + Sync {
    /// Return the filesystem path (no leading slash)
    fn path(&self) -> &str;

    /// Collect and return the current value
    fn collect(&self) -> anyhow::Result<MetricValue>;
}
```

### Example: Simple Gauge

```rust
use obsfs_core::{MetricProvider, MetricValue};
use std::fs;
use anyhow::Result;

pub struct CpuUsageProvider;

impl MetricProvider for CpuUsageProvider {
    fn path(&self) -> &str {
        "system/cpu/usage"  // Will appear at /obs/system/cpu/usage
    }

    fn collect(&self) -> Result<MetricValue> {
        let stat = fs::read_to_string("/proc/stat")?;

        let first_line = stat.lines().next().ok_or_else(||
            anyhow::anyhow!("no cpu line in /proc/stat")
        )?;

        // Parse CPU stats and calculate usage percentage
        let usage = parse_cpu_usage(first_line)?;

        Ok(MetricValue::Gauge(usage))
    }
}
```

### Example: Counter

```rust
pub struct RequestCountProvider;

impl MetricProvider for RequestCountProvider {
    fn path(&self) -> &str {
        "app/requests/total"
    }

    fn collect(&self) -> Result<MetricValue> {
        let count: u64 = fetch_request_count_from_service()?;
        Ok(MetricValue::Counter(count))
    }
}
```

### Example: Text Value

```rust
pub struct ServiceStatusProvider;

impl MetricProvider for ServiceStatusProvider {
    fn path(&self) -> &str {
        "app/status"
    }

    fn collect(&self) -> Result<MetricValue> {
        let status = query_service_status()?;
        Ok(MetricValue::Text(status))
    }
}
```

### Path Guidelines

- Paths use lowercase, no leading/trailing slashes
- Use `/` to create directory hierarchy: `system/cpu/usage` → `/obs/system/cpu/usage`
- Paths should be descriptive and follow a consistent structure across your plugin

### Error Handling

Use `anyhow::Result<MetricValue>`:

```rust
fn collect(&self) -> Result<MetricValue> {
    let data = std::fs::read_to_string("/proc/something")
        .map_err(|e| anyhow::anyhow!("failed to read proc file: {}", e))?;

    let value: f64 = data.parse()
        .map_err(|_| anyhow::anyhow!("failed to parse value"))?;

    Ok(MetricValue::Gauge(value))
}
```

---

## DynamicHandler Trait: Runtime-Discovered Paths

Use `DynamicHandler` for paths that don't exist at startup and are discovered at runtime:

```rust
pub trait DynamicHandler: Send + Sync {
    /// Path prefix this handler responds to (no leading/trailing slash)
    fn prefix(&self) -> &str;

    /// List all available entries under this prefix
    fn list_entries(&self) -> Vec<String>;

    /// Check if a subpath exists
    fn exists(&self, subpath: &str) -> bool;

    /// Read content for a subpath
    fn read(&self, subpath: &str) -> Option<String>;
}
```

### Example: Process Information Handler

This handler makes `/obs/proc` list all active PIDs and lets you read info for any PID:

```rust
use obsfs_core::DynamicHandler;
use std::fs;

pub struct ProcessHandler;

impl DynamicHandler for ProcessHandler {
    fn prefix(&self) -> &str {
        "proc"  // Handles /obs/proc
    }

    fn list_entries(&self) -> Vec<String> {
        let mut pids = Vec::new();

        if let Ok(entries) = fs::read_dir("/proc") {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.parse::<u32>().is_ok() {
                        pids.push(name.to_string());
                    }
                }
            }
        }

        pids.sort();
        pids
    }

    fn exists(&self, subpath: &str) -> bool {
        // subpath is "1234" for /obs/proc/1234
        if let Ok(pid) = subpath.parse::<u32>() {
            std::path::Path::new(&format!("/proc/{}", pid)).exists()
        } else {
            false
        }
    }

    fn read(&self, subpath: &str) -> Option<String> {
        let pid: u32 = subpath.parse().ok()?;
        let stat = fs::read_to_string(format!("/proc/{}/stat", pid)).ok()?;

        // Parse and format the data
        Some(format_process_stat(&stat))
    }
}
```

### Example: Custom Entries Handler

Dynamic handlers don't have to be PID-based. You could create entries based on any runtime state:

```rust
pub struct NetworkInterfaceHandler;

impl DynamicHandler for NetworkInterfaceHandler {
    fn prefix(&self) -> &str {
        "net/interfaces"  // Handles /obs/net/interfaces
    }

    fn list_entries(&self) -> Vec<String> {
        // Return list of network interfaces (eth0, wlan0, lo, etc.)
        list_network_interfaces()
    }

    fn exists(&self, subpath: &str) -> bool {
        // subpath is "eth0" for /obs/net/interfaces/eth0
        interface_exists(subpath)
    }

    fn read(&self, subpath: &str) -> Option<String> {
        // Return stats for the interface
        get_interface_stats(subpath)
    }
}
```

### Key Points

- **prefix()**: No slashes, used to route paths like `/obs/proc/1234` to the correct handler
- **list_entries()**: Return just the names (`["1", "2", "1234"]`), not full paths
- **exists()**: Quick check - used by filesystem layer for permission checks
- **read()**: Return content as `Option<String>`. Return `None` if the path doesn't exist

---

## MetricValue Types

ObsFS supports three metric types:

### Gauge: Point-in-time Numeric Measurement

Used for values that fluctuate (CPU %, memory %, temperature):

```rust
MetricValue::Gauge(42.5)   // Will format as "42.50"
```

Output:
```bash
$ cat /obs/system/cpu/usage
42.50
```

### Counter: Monotonically Increasing Count

Used for cumulative values (total requests, bytes sent):

```rust
MetricValue::Counter(1234567)   // Will format as "1234567"
```

Output:
```bash
$ cat /obs/app/requests/total
1234567
```

### Text: String Value

Used for status, names, detailed information:

```rust
MetricValue::Text("healthy".to_string())
```

Output:
```bash
$ cat /obs/app/status
healthy
```

### Formatting

Values are automatically formatted based on output mode:

**Plain text** (default):
```bash
$ cat /obs/metric
42.50
```

**JSON** (write `json` to `/obs/_meta/format`):
```bash
$ echo json > /obs/_meta/format
$ cat /obs/metric
{"value":42.5,"type":"gauge","timestamp":"2026-04-07T12:34:56Z"}
```

---

## Error Handling

Use `anyhow::Result<MetricValue>` for all errors:

```rust
use anyhow::{Result, anyhow, Context};
use std::fs;

fn collect(&self) -> Result<MetricValue> {
    // Map errors with context
    let content = fs::read_to_string("/proc/stat")
        .context("failed to read /proc/stat")?;

    // Manual error construction
    let value = parse_value(&content)
        .ok_or_else(|| anyhow!("invalid stat format"))?;

    Ok(MetricValue::Gauge(value))
}
```

The filesystem layer will:
- Log the error
- Return an I/O error to the client
- Continue running (errors in one metric don't crash the daemon)

---

## Testing Your Provider

Test providers directly without FUSE:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collect_returns_valid_value() {
        let provider = CpuUsageProvider;
        let result = provider.collect().unwrap();

        match result {
            MetricValue::Gauge(value) => {
                assert!(value >= 0.0 && value <= 100.0);
            }
            _ => panic!("expected gauge"),
        }
    }

    #[test]
    fn test_path_is_correct() {
        let provider = CpuUsageProvider;
        assert_eq!(provider.path(), "system/cpu/usage");
    }

    #[test]
    fn test_collect_error_handling() {
        // If your provider reads files, mock them:
        // Use tempfile crate to create test files
        let provider = MyProvider::new("/tmp/test");
        let result = provider.collect();
        assert!(result.is_err());
    }
}
```

For dynamic handlers:

```rust
#[test]
fn test_dynamic_handler_list_entries() {
    let handler = ProcessHandler;
    let entries = handler.list_entries();

    assert!(!entries.is_empty());
    assert!(entries.contains(&"1".to_string())); // init process
}

#[test]
fn test_dynamic_handler_exists() {
    let handler = ProcessHandler;

    assert!(handler.exists("1")); // init always exists
    assert!(!handler.exists("999999")); // unlikely to exist
}
```

---

## Registration: Wiring Up in main.rs

Plugins must be registered in the main application. Find `obsfs-cli/src/main.rs`:

```rust
use obsfs_plugins::health::HealthPlugin;
use obsfs_plugins::proc_info::ProcessInfoPlugin;
use obsfs_plugins::my_new_plugin::MyNewPlugin;  // Add your plugin

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ... setup code ...

    // Create and register plugins
    let plugins: Vec<Arc<dyn Plugin>> = vec![
        Arc::new(HealthPlugin::new()),
        Arc::new(ProcessInfoPlugin::new()),
        Arc::new(MyNewPlugin::new()),  // Register your plugin
    ];

    // Register each plugin
    for plugin in &plugins {
        tracing::info!("Loading plugin: {}", plugin.name());
        plugin.register(&mut registry)?;
    }

    // Register dynamic handlers
    for plugin in &plugins {
        for handler in plugin.dynamic_handlers() {
            fs.register_dynamic_handler(handler);
        }
    }

    // ... rest of startup ...

    Ok(())
}
```

---

## Complete Example: Custom Plugin

Here's a complete example plugin that monitors disk usage:

**File: `crates/obsfs-plugins/src/disk/mod.rs`**

```rust
use std::fs;
use std::sync::Arc;
use anyhow::Result;
use obsfs_core::{MetricProvider, MetricValue, Plugin, Registry};

/// Disk usage for a specific mount point
pub struct DiskUsageProvider {
    mount: String,
}

impl DiskUsageProvider {
    pub fn new(mount: &str) -> Self {
        Self {
            mount: mount.to_string(),
        }
    }
}

impl MetricProvider for DiskUsageProvider {
    fn path(&self) -> &str {
        "storage/disk/usage"  // Simplified for example
    }

    fn collect(&self) -> Result<MetricValue> {
        let path = std::ffi::CString::new(self.mount.clone())?;

        let mut stat: std::mem::MaybeUninit<libc::statvfs> =
            std::mem::MaybeUninit::uninit();

        let result = unsafe { libc::statvfs(path.as_ptr(), stat.as_mut_ptr()) };

        if result != 0 {
            anyhow::bail!("failed to stat filesystem");
        }

        let stat = unsafe { stat.assume_init() };
        let total = stat.f_blocks as f64 * stat.f_frsize as f64;
        let available = stat.f_bavail as f64 * stat.f_frsize as f64;
        let used = total - available;

        let percent = if total > 0.0 {
            (used / total) * 100.0
        } else {
            0.0
        };

        Ok(MetricValue::Gauge(percent))
    }
}

/// Plugin for disk usage monitoring
pub struct DiskPlugin {
    mount: String,
}

impl DiskPlugin {
    pub fn new(mount: &str) -> Self {
        Self {
            mount: mount.to_string(),
        }
    }
}

impl Plugin for DiskPlugin {
    fn name(&self) -> &str {
        "disk"
    }

    fn description(&self) -> &str {
        "Monitor disk usage for mounted filesystems"
    }

    fn register(&self, registry: &mut Registry) -> Result<()> {
        registry.insert_provider(Arc::new(
            DiskUsageProvider::new(&self.mount)
        ))?;

        tracing::info!("Registered disk plugin for {}", self.mount);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disk_provider_path() {
        let provider = DiskUsageProvider::new("/");
        assert_eq!(provider.path(), "storage/disk/usage");
    }

    #[test]
    fn test_disk_provider_collect() {
        // Will work on any system with a root filesystem
        let provider = DiskUsageProvider::new("/");
        let result = provider.collect();

        assert!(result.is_ok());
        match result.unwrap() {
            MetricValue::Gauge(usage) => {
                assert!(usage >= 0.0 && usage <= 100.0);
            }
            _ => panic!("expected gauge"),
        }
    }
}
```

Usage:
```bash
$ cat /obs/storage/disk/usage
47.23
```

---

## Best Practices

### 1. Thread Safety

All providers and handlers must be `Send + Sync`:

```rust
// Good: simple data, no mutable state
pub struct MyProvider;

// Good: uses Arc for shared state
pub struct MyProvider {
    cache: Arc<RwLock<HashMap<String, Value>>>,
}

// Avoid: mutable fields in providers
pub struct BadProvider {
    state: Mutex<Vec<String>>,  // Will cause issues
}
```

### 2. Error Recovery

Metrics failing shouldn't crash the daemon:

```rust
fn collect(&self) -> Result<MetricValue> {
    // Return errors gracefully, don't panic
    match std::fs::read_to_string("/proc/something") {
        Ok(content) => parse_value(&content),
        Err(e) => {
            // Log but don't panic
            tracing::warn!("failed to read metric: {}", e);
            Ok(MetricValue::Gauge(0.0))  // Default value
        }
    }
}
```

### 3. Performance

Collect frequently:

```rust
// Good: fast, cached operations
fn collect(&self) -> Result<MetricValue> {
    let value = std::fs::read_to_string("/proc/stat")?;
    Ok(MetricValue::Gauge(parse_quick(&value)?))
}

// Avoid: expensive operations
fn collect(&self) -> Result<MetricValue> {
    // Don't make network calls, spawn processes, etc.
    // If you must, cache the result
    Ok(MetricValue::Gauge(0.0))
}
```

### 4. Path Consistency

Use hierarchical paths matching your metric structure:

```rust
system/cpu/cores          // ← Parent: system > cpu
system/cpu/usage          //   Siblings: all under system/cpu
system/memory/total
system/memory/available

app/database/connections   // ← Clear hierarchy
app/database/pool_size
app/cache/hit_rate
```

### 5. Documentation

Include examples in doc comments:

```rust
/// Provides CPU usage percentage (0-100).
///
/// Reads /proc/stat to calculate usage of all CPUs.
///
/// # Example
///
/// ```ignore
/// $ cat /obs/system/cpu/usage
/// 42.50
/// ```
pub struct CpuUsageProvider;
```

---

## Troubleshooting

### Metric Not Appearing

**Problem**: Registered a metric but `/obs/path` doesn't exist

**Solutions**:
- Check the plugin's `register()` is called (look at startup logs)
- Verify the plugin is listed in `main.rs`
- Ensure provider's `path()` matches what you expect
- Run `ls /obs/` to verify the path structure

### "path conflict" Error

**Problem**: Error during registration: "path conflict: 'system' exists and is not a directory"

**Solutions**:
- Ensure you're not creating a file where a directory should be
- Use consistent path hierarchies (e.g., `system/cpu/usage` not both `system` and `system/cpu/usage`)

### Dynamic Handler Not Listing Entries

**Problem**: `ls /obs/proc/` shows nothing or wrong entries

**Solutions**:
- Check `list_entries()` returns the right data
- Verify the handler's `prefix()` matches the mounted path
- Test `list_entries()` directly in unit tests

---

## References

- **Core types**: `crates/obsfs-core/src/types.rs`
- **Plugin trait**: `crates/obsfs-core/src/plugin.rs`
- **Registry**: `crates/obsfs-core/src/registry.rs`
- **Example: Health plugin**: `crates/obsfs-plugins/src/health/mod.rs`
- **Example: Proc info**: `crates/obsfs-plugins/src/proc_info/mod.rs`
