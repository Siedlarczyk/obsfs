//! Filesystem usage metrics using statvfs.

use obsfs_core::{MetricProvider, MetricValue};
use std::ffi::CString;
use std::fs;
use std::mem::MaybeUninit;

/// Discovers mounted filesystems from /proc/mounts.
pub fn discover_filesystems(proc_path: &str) -> Vec<(String, String)> {
    let mounts_path = format!("{}/mounts", proc_path);
    let contents = match fs::read_to_string(&mounts_path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };

    contents
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let mount_point = parts[1];
                let fs_type = parts[2];

                // Only include real filesystems
                if is_real_filesystem(fs_type, mount_point) {
                    let name = sanitize_mount_name(mount_point);
                    return Some((name, mount_point.to_string()));
                }
            }
            None
        })
        .collect()
}

fn is_real_filesystem(fs_type: &str, mount_point: &str) -> bool {
    // Exclude virtual/pseudo filesystems
    let virtual_fs = [
        "proc",
        "sysfs",
        "devtmpfs",
        "devpts",
        "tmpfs",
        "cgroup",
        "cgroup2",
        "pstore",
        "securityfs",
        "debugfs",
        "fusectl",
        "mqueue",
        "hugetlbfs",
        "rpc_pipefs",
        "nfsd",
        "binfmt_misc",
        "autofs",
        "overlay",
        "squashfs",
    ];

    if virtual_fs.contains(&fs_type) {
        return false;
    }

    // Exclude special mount points
    if mount_point.starts_with("/proc")
        || mount_point.starts_with("/sys")
        || mount_point.starts_with("/dev")
        || mount_point.starts_with("/run")
        || mount_point.starts_with("/snap")
    {
        return false;
    }

    true
}

fn sanitize_mount_name(mount_point: &str) -> String {
    if mount_point == "/" {
        "root".to_string()
    } else {
        mount_point.trim_start_matches('/').replace('/', "_")
    }
}

#[derive(Debug)]
struct FsStats {
    total: u64,
    available: u64,
    used: u64,
}

fn get_fs_stats(mount_point: &str) -> anyhow::Result<FsStats> {
    let c_path =
        CString::new(mount_point).map_err(|_| anyhow::anyhow!("invalid mount point path"))?;

    let mut stat: MaybeUninit<libc::statvfs> = MaybeUninit::uninit();

    let result = unsafe { libc::statvfs(c_path.as_ptr(), stat.as_mut_ptr()) };

    if result != 0 {
        return Err(anyhow::anyhow!(
            "statvfs failed for {}: {}",
            mount_point,
            std::io::Error::last_os_error()
        ));
    }

    let stat = unsafe { stat.assume_init() };

    let block_size = stat.f_frsize as u64;
    let total = stat.f_blocks as u64 * block_size;
    let available = stat.f_bavail as u64 * block_size;
    let free = stat.f_bfree as u64 * block_size;
    let used = total - free;

    Ok(FsStats {
        total,
        available,
        used,
    })
}

/// Provides total bytes for a filesystem.
#[derive(Debug)]
pub struct FsTotalProvider {
    mount_point: String,
    metric_path: String,
}

impl FsTotalProvider {
    pub fn new(name: &str, mount_point: &str) -> Self {
        Self {
            mount_point: mount_point.to_string(),
            metric_path: format!("system/fs/{}/total", name),
        }
    }
}

impl MetricProvider for FsTotalProvider {
    fn path(&self) -> &str {
        &self.metric_path
    }

    fn collect(&self) -> anyhow::Result<MetricValue> {
        let stats = get_fs_stats(&self.mount_point)?;
        Ok(MetricValue::Counter(stats.total))
    }
}

/// Provides used bytes for a filesystem.
#[derive(Debug)]
pub struct FsUsedProvider {
    mount_point: String,
    metric_path: String,
}

impl FsUsedProvider {
    pub fn new(name: &str, mount_point: &str) -> Self {
        Self {
            mount_point: mount_point.to_string(),
            metric_path: format!("system/fs/{}/used", name),
        }
    }
}

impl MetricProvider for FsUsedProvider {
    fn path(&self) -> &str {
        &self.metric_path
    }

    fn collect(&self) -> anyhow::Result<MetricValue> {
        let stats = get_fs_stats(&self.mount_point)?;
        Ok(MetricValue::Counter(stats.used))
    }
}

/// Provides available bytes for a filesystem.
#[derive(Debug)]
pub struct FsAvailableProvider {
    mount_point: String,
    metric_path: String,
}

impl FsAvailableProvider {
    pub fn new(name: &str, mount_point: &str) -> Self {
        Self {
            mount_point: mount_point.to_string(),
            metric_path: format!("system/fs/{}/available", name),
        }
    }
}

impl MetricProvider for FsAvailableProvider {
    fn path(&self) -> &str {
        &self.metric_path
    }

    fn collect(&self) -> anyhow::Result<MetricValue> {
        let stats = get_fs_stats(&self.mount_point)?;
        Ok(MetricValue::Counter(stats.available))
    }
}

/// Provides usage percentage for a filesystem.
#[derive(Debug)]
pub struct FsPercentUsedProvider {
    mount_point: String,
    metric_path: String,
}

impl FsPercentUsedProvider {
    pub fn new(name: &str, mount_point: &str) -> Self {
        Self {
            mount_point: mount_point.to_string(),
            metric_path: format!("system/fs/{}/percent_used", name),
        }
    }
}

impl MetricProvider for FsPercentUsedProvider {
    fn path(&self) -> &str {
        &self.metric_path
    }

    fn collect(&self) -> anyhow::Result<MetricValue> {
        let stats = get_fs_stats(&self.mount_point)?;
        let percent = if stats.total > 0 {
            100.0 * (stats.used as f64 / stats.total as f64)
        } else {
            0.0
        };
        Ok(MetricValue::Gauge(percent))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_mount_name() {
        assert_eq!(sanitize_mount_name("/"), "root");
        assert_eq!(sanitize_mount_name("/home"), "home");
        assert_eq!(sanitize_mount_name("/mnt/data"), "mnt_data");
    }

    #[test]
    fn test_is_real_filesystem() {
        assert!(is_real_filesystem("ext4", "/"));
        assert!(is_real_filesystem("xfs", "/home"));
        assert!(is_real_filesystem("btrfs", "/data"));

        assert!(!is_real_filesystem("proc", "/proc"));
        assert!(!is_real_filesystem("sysfs", "/sys"));
        assert!(!is_real_filesystem("tmpfs", "/run"));
    }
}
