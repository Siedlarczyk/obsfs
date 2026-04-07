//! CPU metrics from /proc/stat and /proc/loadavg.

use obsfs_core::{MetricProvider, MetricValue};
use std::fs;

/// Provides CPU usage percentage from /proc/stat.
#[derive(Debug)]
pub struct CpuUsageProvider {
    proc_path: String,
}

impl CpuUsageProvider {
    pub fn new(proc_path: String) -> Self {
        Self { proc_path }
    }

    fn parse_cpu_stat(&self) -> anyhow::Result<(u64, u64)> {
        let stat_path = format!("{}/stat", self.proc_path);
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
                let idle_total = idle + iowait;

                return Ok((total, idle_total));
            }
        }

        Err(anyhow::anyhow!("no cpu line found in /proc/stat"))
    }
}

impl MetricProvider for CpuUsageProvider {
    fn path(&self) -> &str {
        "system/cpu/usage"
    }

    fn collect(&self) -> anyhow::Result<MetricValue> {
        let (total, idle) = self.parse_cpu_stat()?;

        let usage = if total > 0 {
            100.0 * (1.0 - (idle as f64 / total as f64))
        } else {
            0.0
        };

        Ok(MetricValue::Gauge(usage))
    }
}

/// Provides a load average value (1m, 5m, or 15m).
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

/// Provides system uptime as a human-readable string.
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
