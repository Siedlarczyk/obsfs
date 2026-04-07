//! CPU metrics from /proc/stat and /proc/loadavg.

use obsfs_core::{MetricProvider, MetricValue};
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tracing::error;

/// CPU statistics snapshot from /proc/stat
#[derive(Debug, Clone, Copy)]
struct CpuStat {
    total: u64,
    idle: u64,
}

/// Uses a background thread to sample CPU stats every 500ms and calculate
/// the actual CPU usage based on delta between consecutive snapshots.
pub struct CpuUsageProvider {
    proc_path: String,
    /// Stores the last calculated usage as f64 bits (lock-free)
    last_usage: Arc<AtomicU64>,
}

impl CpuUsageProvider {
    pub fn new(proc_path: String) -> Self {
        let last_usage = Arc::new(AtomicU64::new(0u64));
        let last_usage_clone = Arc::clone(&last_usage);
        let proc_path_clone = proc_path.clone();

        // Start background sampling thread
        thread::spawn(move || {
            Self::sampling_thread(proc_path_clone, last_usage_clone);
        });

        Self {
            proc_path,
            last_usage,
        }
    }

    /// Background thread that periodically samples CPU stats and calculates usage
    fn sampling_thread(proc_path: String, last_usage: Arc<AtomicU64>) {
        let mut last_stat: Option<CpuStat> = None;

        loop {
            match Self::parse_cpu_stat_from_path(&proc_path) {
                Ok(current_stat) => {
                    if let Some(prev_stat) = last_stat {
                        // Calculate delta
                        let delta_total = current_stat.total.saturating_sub(prev_stat.total);
                        let delta_idle = current_stat.idle.saturating_sub(prev_stat.idle);

                        // Calculate usage: 100 * (1 - idle_ratio)
                        let usage = if delta_total > 0 {
                            100.0 * (1.0 - (delta_idle as f64 / delta_total as f64))
                        } else {
                            0.0
                        };

                        // Store as f64 bits for lock-free access
                        let bits = usage.to_bits();
                        last_usage.store(bits, Ordering::Relaxed);
                    }

                    last_stat = Some(current_stat);
                }
                Err(e) => {
                    // Log error but continue sampling
                    error!("failed to parse CPU stats: {}", e);
                }
            }

            thread::sleep(Duration::from_millis(500));
        }
    }

    fn parse_cpu_stat_from_path(proc_path: &str) -> anyhow::Result<CpuStat> {
        let stat_path = format!("{}/stat", proc_path);
        let contents = fs::read_to_string(&stat_path)
            .map_err(|e| anyhow::anyhow!("failed to read {}: {}", stat_path, e))?;

        for line in contents.lines() {
            if line.starts_with("cpu ") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() < 5 {
                    return Err(anyhow::anyhow!("invalid /proc/stat format"));
                }

                let user: u64 = parts[1].parse()?;
                let nice: u64 = parts[2].parse()?;
                let system: u64 = parts[3].parse()?;
                let idle: u64 = parts[4].parse()?;
                let iowait: u64 = parts.get(5).and_then(|s| s.parse().ok()).unwrap_or(0);
                let irq: u64 = parts.get(6).and_then(|s| s.parse().ok()).unwrap_or(0);
                let softirq: u64 = parts.get(7).and_then(|s| s.parse().ok()).unwrap_or(0);
                let steal: u64 = parts.get(8).and_then(|s| s.parse().ok()).unwrap_or(0);

                let total = user + nice + system + idle + iowait + irq + softirq + steal;

                return Ok(CpuStat { total, idle });
            }
        }

        Err(anyhow::anyhow!("no cpu line found in /proc/stat"))
    }
}

impl std::fmt::Debug for CpuUsageProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CpuUsageProvider")
            .field("proc_path", &self.proc_path)
            .finish()
    }
}

impl MetricProvider for CpuUsageProvider {
    fn path(&self) -> &str {
        "system/cpu/usage"
    }

    fn collect(&self) -> anyhow::Result<MetricValue> {
        // Just return the last calculated value (no blocking)
        let bits = self.last_usage.load(Ordering::Relaxed);
        let usage = f64::from_bits(bits);
        Ok(MetricValue::Gauge(usage))
    }
}

#[derive(Debug)]
pub struct LoadAverageProvider {
    proc_path: String,
    metric_path: String,
    index: usize,
}

impl LoadAverageProvider {
    pub fn new(proc_path: String, metric_path: &str, index: usize) -> Self {
        Self {
            proc_path,
            metric_path: metric_path.to_string(),
            index,
        }
    }
}

impl MetricProvider for LoadAverageProvider {
    fn path(&self) -> &str {
        &self.metric_path
    }

    fn collect(&self) -> anyhow::Result<MetricValue> {
        let loadavg_path = format!("{}/loadavg", self.proc_path);
        let contents = fs::read_to_string(&loadavg_path)
            .map_err(|e| anyhow::anyhow!("failed to read {}: {}", loadavg_path, e))?;

        let parts: Vec<&str> = contents.split_whitespace().collect();
        if parts.len() < 3 {
            return Err(anyhow::anyhow!("invalid /proc/loadavg format"));
        }

        let value: f64 = parts
            .get(self.index)
            .ok_or_else(|| anyhow::anyhow!("load average index {} out of range", self.index))?
            .parse()
            .map_err(|e| anyhow::anyhow!("failed to parse load average: {}", e))?;

        Ok(MetricValue::Gauge(value))
    }
}

#[derive(Debug)]
pub struct UptimeProvider {
    proc_path: String,
}

impl UptimeProvider {
    pub fn new(proc_path: String) -> Self {
        Self { proc_path }
    }

    fn format_duration(seconds: u64) -> String {
        let days = seconds / 86400;
        let hours = (seconds % 86400) / 3600;
        let minutes = (seconds % 3600) / 60;
        let secs = seconds % 60;

        let mut parts = Vec::new();

        if days > 0 {
            parts.push(format!("{}d", days));
        }
        if hours > 0 || days > 0 {
            parts.push(format!("{}h", hours));
        }
        if minutes > 0 || hours > 0 || days > 0 {
            parts.push(format!("{}m", minutes));
        }
        parts.push(format!("{}s", secs));

        parts.join(" ")
    }
}

impl MetricProvider for UptimeProvider {
    fn path(&self) -> &str {
        "system/uptime"
    }

    fn collect(&self) -> anyhow::Result<MetricValue> {
        let uptime_path = format!("{}/uptime", self.proc_path);
        let contents = fs::read_to_string(&uptime_path)
            .map_err(|e| anyhow::anyhow!("failed to read {}: {}", uptime_path, e))?;

        let uptime_secs: f64 = contents
            .split_whitespace()
            .next()
            .ok_or_else(|| anyhow::anyhow!("empty /proc/uptime"))?
            .parse()
            .map_err(|e| anyhow::anyhow!("failed to parse uptime: {}", e))?;

        let formatted = Self::format_duration(uptime_secs as u64);
        Ok(MetricValue::Text(formatted))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_duration() {
        assert_eq!(UptimeProvider::format_duration(45), "45s");
        assert_eq!(UptimeProvider::format_duration(125), "2m 5s");
        assert_eq!(UptimeProvider::format_duration(3725), "1h 2m 5s");
        assert_eq!(UptimeProvider::format_duration(90061), "1d 1h 1m 1s");
    }
}
