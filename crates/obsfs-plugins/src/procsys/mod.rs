//! System metrics collector from Linux's `/proc` and `/sys` filesystems.

mod cpu;
mod disk;
mod fs;
mod memory;
mod net;

use std::sync::Arc;
use obsfs_core::{Plugin, Registry};

pub use cpu::{CpuUsageProvider, LoadAverageProvider, UptimeProvider};
pub use disk::{DiskReadBytesProvider, DiskReadIopsProvider, DiskWriteBytesProvider, DiskWriteIopsProvider};
pub use fs::{FsAvailableProvider, FsPercentUsedProvider, FsTotalProvider, FsUsedProvider};
pub use memory::{MemoryMetricProvider, SwapUsedProvider};
pub use net::{NetRxBytesProvider, NetRxPacketsProvider, NetTxBytesProvider, NetTxPacketsProvider};

/// Plugin for system metrics from /proc and /sys.
#[derive(Debug, Default)]
pub struct ProcSysPlugin {
    proc_path: String,
}

impl ProcSysPlugin {
    /// Creates a new plugin using the default /proc path.
    pub fn new() -> Self {
        Self {
            proc_path: "/proc".to_string(),
        }
    }

    /// Creates a new plugin with a custom /proc path (useful for testing).
    pub fn with_proc_path(proc_path: impl Into<String>) -> Self {
        Self {
            proc_path: proc_path.into(),
        }
    }

    fn register_cpu_metrics(&self, registry: &mut Registry) -> anyhow::Result<()> {
        // CPU usage
        registry
            .insert_provider(Arc::new(CpuUsageProvider::new(self.proc_path.clone())))
            .map_err(|e| anyhow::anyhow!(e))?;

        // Uptime
        registry
            .insert_provider(Arc::new(UptimeProvider::new(self.proc_path.clone())))
            .map_err(|e| anyhow::anyhow!(e))?;

        // Load averages
        registry
            .insert_provider(Arc::new(LoadAverageProvider::new(
                self.proc_path.clone(),
                "system/cpu/load_1m",
                0,
            )))
            .map_err(|e| anyhow::anyhow!(e))?;

        registry
            .insert_provider(Arc::new(LoadAverageProvider::new(
                self.proc_path.clone(),
                "system/cpu/load_5m",
                1,
            )))
            .map_err(|e| anyhow::anyhow!(e))?;

        registry
            .insert_provider(Arc::new(LoadAverageProvider::new(
                self.proc_path.clone(),
                "system/cpu/load_15m",
                2,
            )))
            .map_err(|e| anyhow::anyhow!(e))?;

        Ok(())
    }

    fn register_memory_metrics(&self, registry: &mut Registry) -> anyhow::Result<()> {
        let metrics = [
            ("system/memory/total", "MemTotal"),
            ("system/memory/available", "MemAvailable"),
            ("system/memory/free", "MemFree"),
            ("system/memory/swap_total", "SwapTotal"),
        ];

        for (path, field) in metrics {
            registry
                .insert_provider(Arc::new(MemoryMetricProvider::new(
                    self.proc_path.clone(),
                    path,
                    field,
                )))
                .map_err(|e| anyhow::anyhow!(e))?;
        }

        registry
            .insert_provider(Arc::new(SwapUsedProvider::new(self.proc_path.clone())))
            .map_err(|e| anyhow::anyhow!(e))?;

        Ok(())
    }

    fn register_disk_metrics(&self, registry: &mut Registry) -> anyhow::Result<()> {
        let devices = disk::discover_devices(&self.proc_path);

        for device in devices {
            registry
                .insert_provider(Arc::new(DiskReadBytesProvider::new(
                    self.proc_path.clone(),
                    &device,
                )))
                .map_err(|e| anyhow::anyhow!(e))?;

            registry
                .insert_provider(Arc::new(DiskWriteBytesProvider::new(
                    self.proc_path.clone(),
                    &device,
                )))
                .map_err(|e| anyhow::anyhow!(e))?;

            registry
                .insert_provider(Arc::new(DiskReadIopsProvider::new(
                    self.proc_path.clone(),
                    &device,
                )))
                .map_err(|e| anyhow::anyhow!(e))?;

            registry
                .insert_provider(Arc::new(DiskWriteIopsProvider::new(
                    self.proc_path.clone(),
                    &device,
                )))
                .map_err(|e| anyhow::anyhow!(e))?;

            tracing::debug!(device = %device, "Registered disk metrics");
        }

        Ok(())
    }

    fn register_fs_metrics(&self, registry: &mut Registry) -> anyhow::Result<()> {
        let filesystems = fs::discover_filesystems(&self.proc_path);

        for (name, mount_point) in filesystems {
            registry
                .insert_provider(Arc::new(FsTotalProvider::new(&name, &mount_point)))
                .map_err(|e| anyhow::anyhow!(e))?;

            registry
                .insert_provider(Arc::new(FsUsedProvider::new(&name, &mount_point)))
                .map_err(|e| anyhow::anyhow!(e))?;

            registry
                .insert_provider(Arc::new(FsAvailableProvider::new(&name, &mount_point)))
                .map_err(|e| anyhow::anyhow!(e))?;

            registry
                .insert_provider(Arc::new(FsPercentUsedProvider::new(&name, &mount_point)))
                .map_err(|e| anyhow::anyhow!(e))?;

            tracing::debug!(name = %name, mount_point = %mount_point, "Registered filesystem metrics");
        }

        Ok(())
    }

    fn register_net_metrics(&self, registry: &mut Registry) -> anyhow::Result<()> {
        let interfaces = net::discover_interfaces(&self.proc_path);

        for interface in interfaces {
            registry
                .insert_provider(Arc::new(NetRxBytesProvider::new(
                    self.proc_path.clone(),
                    &interface,
                )))
                .map_err(|e| anyhow::anyhow!(e))?;

            registry
                .insert_provider(Arc::new(NetTxBytesProvider::new(
                    self.proc_path.clone(),
                    &interface,
                )))
                .map_err(|e| anyhow::anyhow!(e))?;

            registry
                .insert_provider(Arc::new(NetRxPacketsProvider::new(
                    self.proc_path.clone(),
                    &interface,
                )))
                .map_err(|e| anyhow::anyhow!(e))?;

            registry
                .insert_provider(Arc::new(NetTxPacketsProvider::new(
                    self.proc_path.clone(),
                    &interface,
                )))
                .map_err(|e| anyhow::anyhow!(e))?;

            tracing::debug!(interface = %interface, "Registered network metrics");
        }

        Ok(())
    }
}

impl Plugin for ProcSysPlugin {
    fn name(&self) -> &str {
        "procsys"
    }

    fn description(&self) -> &str {
        "System metrics from /proc and /sys (CPU, memory, disk, network)"
    }

    fn register(&self, registry: &mut Registry) -> anyhow::Result<()> {
        self.register_cpu_metrics(registry)?;
        self.register_memory_metrics(registry)?;
        self.register_disk_metrics(registry)?;
        self.register_fs_metrics(registry)?;
        self.register_net_metrics(registry)?;
        Ok(())
    }
}

/// Type alias for backwards compatibility.
#[deprecated(since = "0.2.0", note = "Use ProcSysPlugin instead")]
pub type ProcSysCollector = ProcSysPlugin;

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs as stdfs;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_mock_proc() -> TempDir {
        let dir = TempDir::new().unwrap();

        // /proc/stat
        let stat_content = "cpu  10000 100 5000 80000 1000 50 25 10 0 0\n";
        let mut f = stdfs::File::create(dir.path().join("stat")).unwrap();
        f.write_all(stat_content.as_bytes()).unwrap();

        // /proc/meminfo
        let meminfo_content = "MemTotal:       16384000 kB\n\
                              MemFree:         1234567 kB\n\
                              MemAvailable:   12345678 kB\n\
                              SwapTotal:       8192000 kB\n\
                              SwapFree:        8000000 kB\n";
        let mut f = stdfs::File::create(dir.path().join("meminfo")).unwrap();
        f.write_all(meminfo_content.as_bytes()).unwrap();

        // /proc/uptime
        let mut f = stdfs::File::create(dir.path().join("uptime")).unwrap();
        f.write_all(b"12345.67 8901.23\n").unwrap();

        // /proc/loadavg
        let mut f = stdfs::File::create(dir.path().join("loadavg")).unwrap();
        f.write_all(b"0.15 0.10 0.05 1/234 5678\n").unwrap();

        // /proc/diskstats
        let diskstats = "   8       0 sda 1000 0 20000 100 500 0 10000 50 0 100 150\n";
        let mut f = stdfs::File::create(dir.path().join("diskstats")).unwrap();
        f.write_all(diskstats.as_bytes()).unwrap();

        // /proc/mounts
        let mounts = "/dev/sda1 / ext4 rw 0 0\n";
        let mut f = stdfs::File::create(dir.path().join("mounts")).unwrap();
        f.write_all(mounts.as_bytes()).unwrap();

        // /proc/net/dev
        stdfs::create_dir_all(dir.path().join("net")).unwrap();
        let netdev = "Inter-|   Receive\n\
                      face |bytes\n\
                        lo: 1000 100 0 0 0 0 0 0 2000 200 0 0 0 0 0 0\n\
                      eth0: 5000 500 0 0 0 0 0 0 3000 300 0 0 0 0 0 0\n";
        let mut f = stdfs::File::create(dir.path().join("net/dev")).unwrap();
        f.write_all(netdev.as_bytes()).unwrap();

        dir
    }

    #[test]
    fn test_plugin_registers_cpu_metrics() {
        let mock_proc = create_mock_proc();
        let plugin = ProcSysPlugin::with_proc_path(mock_proc.path().to_string_lossy().to_string());

        let mut registry = Registry::new();
        plugin.register(&mut registry).unwrap();

        assert!(registry.exists("system/cpu/usage"));
        assert!(registry.exists("system/cpu/load_1m"));
        assert!(registry.exists("system/cpu/load_5m"));
        assert!(registry.exists("system/cpu/load_15m"));
        assert!(registry.exists("system/uptime"));
    }

    #[test]
    fn test_plugin_registers_memory_metrics() {
        let mock_proc = create_mock_proc();
        let plugin = ProcSysPlugin::with_proc_path(mock_proc.path().to_string_lossy().to_string());

        let mut registry = Registry::new();
        plugin.register(&mut registry).unwrap();

        assert!(registry.exists("system/memory/total"));
        assert!(registry.exists("system/memory/available"));
        assert!(registry.exists("system/memory/free"));
        assert!(registry.exists("system/memory/swap_total"));
        assert!(registry.exists("system/memory/swap_used"));
    }

    #[test]
    fn test_plugin_registers_disk_metrics() {
        let mock_proc = create_mock_proc();
        let plugin = ProcSysPlugin::with_proc_path(mock_proc.path().to_string_lossy().to_string());

        let mut registry = Registry::new();
        plugin.register(&mut registry).unwrap();

        assert!(registry.exists("system/disk/sda/read_bytes"));
        assert!(registry.exists("system/disk/sda/write_bytes"));
        assert!(registry.exists("system/disk/sda/read_iops"));
        assert!(registry.exists("system/disk/sda/write_iops"));
    }

    #[test]
    fn test_plugin_registers_net_metrics() {
        let mock_proc = create_mock_proc();
        let plugin = ProcSysPlugin::with_proc_path(mock_proc.path().to_string_lossy().to_string());

        let mut registry = Registry::new();
        plugin.register(&mut registry).unwrap();

        assert!(registry.exists("system/net/eth0/rx_bytes"));
        assert!(registry.exists("system/net/eth0/tx_bytes"));
        assert!(registry.exists("system/net/eth0/rx_packets"));
        assert!(registry.exists("system/net/eth0/tx_packets"));
    }

    #[test]
    fn test_plugin_metadata() {
        let plugin = ProcSysPlugin::new();
        assert_eq!(plugin.name(), "procsys");
        assert!(!plugin.description().is_empty());
        assert!(plugin.dynamic_handlers().is_empty());
    }
}
