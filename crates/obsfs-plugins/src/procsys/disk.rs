//! Disk I/O metrics from /proc/diskstats.
//!
//! Format of /proc/diskstats:
//! major minor name reads_completed reads_merged sectors_read ms_reading
//! writes_completed writes_merged sectors_written ms_writing ios_in_progress
//! ms_io weighted_ms_io
//!
//! Sector size is typically 512 bytes.

use obsfs_core::{MetricProvider, MetricValue};
use std::fs;

const SECTOR_SIZE: u64 = 512;

/// Discovers available block devices from /proc/diskstats.
pub fn discover_devices(proc_path: &str) -> Vec<String> {
    let diskstats_path = format!("{}/diskstats", proc_path);
    let contents = match fs::read_to_string(&diskstats_path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };

    contents
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let name = parts[2];
                // Filter out partitions (keep only whole disks like sda, nvme0n1)
                // Simple heuristic: exclude names ending with a digit that follows a letter
                if is_whole_disk(name) {
                    return Some(name.to_string());
                }
            }
            None
        })
        .collect()
}

/// Check if a device name represents a whole disk (not a partition).
fn is_whole_disk(name: &str) -> bool {
    // Common patterns:
    // sda, sdb (whole disk) vs sda1, sda2 (partitions)
    // nvme0n1 (whole disk) vs nvme0n1p1 (partitions)
    // vda (whole disk) vs vda1 (partitions)

    if name.starts_with("loop") || name.starts_with("ram") || name.starts_with("dm-") {
        return false;
    }

    // nvme: whole disk is nvme0n1, partition is nvme0n1p1
    if name.starts_with("nvme") {
        return !name.contains('p') || name.ends_with("n1");
    }

    // sd*, vd*, hd*: partition ends with digit
    if name.starts_with("sd") || name.starts_with("vd") || name.starts_with("hd") {
        let last_char = name.chars().last().unwrap_or('0');
        return !last_char.is_ascii_digit();
    }

    true
}

#[derive(Debug)]
struct DiskStats {
    reads_completed: u64,
    sectors_read: u64,
    writes_completed: u64,
    sectors_written: u64,
}

fn parse_diskstats(proc_path: &str, device: &str) -> anyhow::Result<DiskStats> {
    let diskstats_path = format!("{}/diskstats", proc_path);
    let contents = fs::read_to_string(&diskstats_path)
        .map_err(|e| anyhow::anyhow!("failed to read {}: {}", diskstats_path, e))?;

    for line in contents.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 14 && parts[2] == device {
            return Ok(DiskStats {
                reads_completed: parts[3].parse().unwrap_or(0),
                sectors_read: parts[5].parse().unwrap_or(0),
                writes_completed: parts[6].parse().unwrap_or(0),
                sectors_written: parts[9].parse().unwrap_or(0),
            });
        }
    }

    Err(anyhow::anyhow!("device {} not found in diskstats", device))
}

/// Provides bytes read for a disk device.
#[derive(Debug)]
pub struct DiskReadBytesProvider {
    proc_path: String,
    device: String,
    metric_path: String,
}

impl DiskReadBytesProvider {
    pub fn new(proc_path: String, device: &str) -> Self {
        Self {
            proc_path,
            metric_path: format!("system/disk/{}/read_bytes", device),
            device: device.to_string(),
        }
    }
}

impl MetricProvider for DiskReadBytesProvider {
    fn path(&self) -> &str {
        &self.metric_path
    }

    fn collect(&self) -> anyhow::Result<MetricValue> {
        let stats = parse_diskstats(&self.proc_path, &self.device)?;
        Ok(MetricValue::Counter(stats.sectors_read * SECTOR_SIZE))
    }
}

/// Provides bytes written for a disk device.
#[derive(Debug)]
pub struct DiskWriteBytesProvider {
    proc_path: String,
    device: String,
    metric_path: String,
}

impl DiskWriteBytesProvider {
    pub fn new(proc_path: String, device: &str) -> Self {
        Self {
            proc_path,
            metric_path: format!("system/disk/{}/write_bytes", device),
            device: device.to_string(),
        }
    }
}

impl MetricProvider for DiskWriteBytesProvider {
    fn path(&self) -> &str {
        &self.metric_path
    }

    fn collect(&self) -> anyhow::Result<MetricValue> {
        let stats = parse_diskstats(&self.proc_path, &self.device)?;
        Ok(MetricValue::Counter(stats.sectors_written * SECTOR_SIZE))
    }
}

/// Provides read IOPS (completed reads) for a disk device.
#[derive(Debug)]
pub struct DiskReadIopsProvider {
    proc_path: String,
    device: String,
    metric_path: String,
}

impl DiskReadIopsProvider {
    pub fn new(proc_path: String, device: &str) -> Self {
        Self {
            proc_path,
            metric_path: format!("system/disk/{}/read_iops", device),
            device: device.to_string(),
        }
    }
}

impl MetricProvider for DiskReadIopsProvider {
    fn path(&self) -> &str {
        &self.metric_path
    }

    fn collect(&self) -> anyhow::Result<MetricValue> {
        let stats = parse_diskstats(&self.proc_path, &self.device)?;
        Ok(MetricValue::Counter(stats.reads_completed))
    }
}

/// Provides write IOPS (completed writes) for a disk device.
#[derive(Debug)]
pub struct DiskWriteIopsProvider {
    proc_path: String,
    device: String,
    metric_path: String,
}

impl DiskWriteIopsProvider {
    pub fn new(proc_path: String, device: &str) -> Self {
        Self {
            proc_path,
            metric_path: format!("system/disk/{}/write_iops", device),
            device: device.to_string(),
        }
    }
}

impl MetricProvider for DiskWriteIopsProvider {
    fn path(&self) -> &str {
        &self.metric_path
    }

    fn collect(&self) -> anyhow::Result<MetricValue> {
        let stats = parse_diskstats(&self.proc_path, &self.device)?;
        Ok(MetricValue::Counter(stats.writes_completed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_whole_disk() {
        assert!(is_whole_disk("sda"));
        assert!(is_whole_disk("sdb"));
        assert!(is_whole_disk("vda"));
        assert!(is_whole_disk("nvme0n1"));

        assert!(!is_whole_disk("sda1"));
        assert!(!is_whole_disk("sda2"));
        assert!(!is_whole_disk("vda1"));
        assert!(!is_whole_disk("loop0"));
        assert!(!is_whole_disk("dm-0"));
    }
}
