//! # Health Collector - System Health Summary
//!
//! Provides an aggregated health summary of the system in a single file.
//! Shows CPU, memory, swap, and disk usage with status indicators.
//!
//! Usage: `cat /obs/health`

use std::fs;

use anyhow::Result;
use obsfs_core::{MetricProvider, MetricValue, Plugin, Registry};
use std::sync::Arc;

// =============================================================================
// THRESHOLDS
// =============================================================================

/// Thresholds for determining health status
struct Thresholds {
    /// CPU load / cores ratio
    cpu_warn: f64,      // 0.7 = 70% of cores
    cpu_critical: f64,  // 1.0 = 100% of cores

    /// Memory usage percentage
    mem_warn: f64,      // 80%
    mem_critical: f64,  // 95%

    /// Swap usage - any usage is warning
    swap_warn: f64,     // 10%
    swap_critical: f64, // 50%

    /// Disk usage percentage
    disk_warn: f64,     // 80%
    disk_critical: f64, // 95%
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            cpu_warn: 0.7,
            cpu_critical: 1.0,
            mem_warn: 80.0,
            mem_critical: 95.0,
            swap_warn: 10.0,
            swap_critical: 50.0,
            disk_warn: 80.0,
            disk_critical: 95.0,
        }
    }
}

// =============================================================================
// STATUS TYPES
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Status {
    Ok,
    Warn,
    Critical,
}

impl Status {
    fn as_str(&self) -> &'static str {
        match self {
            Status::Ok => "OK",
            Status::Warn => "WARN",
            Status::Critical => "CRITICAL",
        }
    }

    fn marker(&self) -> &'static str {
        match self {
            Status::Ok => "",
            Status::Warn => " [WARN]",
            Status::Critical => " [CRITICAL]",
        }
    }
}

// =============================================================================
// HEALTH CHECK RESULT
// =============================================================================

struct CheckResult {
    status: Status,
    line: String,
    issue: Option<String>,
}

// =============================================================================
// HEALTH PROVIDER
// =============================================================================

/// Provides overall system health status with warnings and critical alerts.
pub struct HealthProvider {
    proc_path: String,
    thresholds: Thresholds,
}

impl HealthProvider {
    pub fn new() -> Self {
        Self {
            proc_path: "/proc".to_string(),
            thresholds: Thresholds::default(),
        }
    }

    pub fn with_proc_path(proc_path: impl Into<String>) -> Self {
        Self {
            proc_path: proc_path.into(),
            thresholds: Thresholds::default(),
        }
    }

    /// Check CPU health by comparing load average to core count
    fn check_cpu(&self) -> CheckResult {
        let loadavg_path = format!("{}/loadavg", self.proc_path);
        let cpuinfo_path = format!("{}/cpuinfo", self.proc_path);

        // Read load average
        let load_1m = fs::read_to_string(&loadavg_path)
            .ok()
            .and_then(|s| s.split_whitespace().next().and_then(|v| v.parse::<f64>().ok()))
            .unwrap_or(0.0);

        // Count number of cores
        let cores = fs::read_to_string(&cpuinfo_path)
            .map(|s| s.matches("processor").count())
            .unwrap_or(1) as f64;

        let ratio = load_1m / cores;
        let percent = (ratio * 100.0) as u32;

        let status = if ratio >= self.thresholds.cpu_critical {
            Status::Critical
        } else if ratio >= self.thresholds.cpu_warn {
            Status::Warn
        } else {
            Status::Ok
        };

        let line = format!(
            "cpu:      {}% (load {:.2} on {} cores){}",
            percent.min(999),
            load_1m,
            cores as u32,
            status.marker()
        );

        let issue = if status != Status::Ok {
            Some(format!(
                "cpu: load {:.1} is {:.1}x cores ({})",
                load_1m, ratio, cores as u32
            ))
        } else {
            None
        };

        CheckResult { status, line, issue }
    }

    /// Check memory health based on available percentage
    fn check_memory(&self) -> CheckResult {
        let meminfo_path = format!("{}/meminfo", self.proc_path);

        let content = fs::read_to_string(&meminfo_path).unwrap_or_default();

        let mut total_kb = 0u64;
        let mut available_kb = 0u64;

        for line in content.lines() {
            if line.starts_with("MemTotal:") {
                total_kb = Self::parse_meminfo_value(line);
            } else if line.starts_with("MemAvailable:") {
                available_kb = Self::parse_meminfo_value(line);
            }
        }

        let used_kb = total_kb.saturating_sub(available_kb);
        let percent = if total_kb > 0 {
            (used_kb as f64 / total_kb as f64) * 100.0
        } else {
            0.0
        };

        let status = if percent >= self.thresholds.mem_critical {
            Status::Critical
        } else if percent >= self.thresholds.mem_warn {
            Status::Warn
        } else {
            Status::Ok
        };

        let line = format!(
            "memory:   {:.0}% ({} / {}){}",
            percent,
            Self::format_bytes(used_kb * 1024),
            Self::format_bytes(total_kb * 1024),
            status.marker()
        );

        let issue = if status != Status::Ok {
            Some(format!(
                "memory: {:.0}% used, only {} available",
                percent,
                Self::format_bytes(available_kb * 1024)
            ))
        } else {
            None
        };

        CheckResult { status, line, issue }
    }

    /// Check swap health; any usage is a warning
    fn check_swap(&self) -> CheckResult {
        let meminfo_path = format!("{}/meminfo", self.proc_path);

        let content = fs::read_to_string(&meminfo_path).unwrap_or_default();

        let mut total_kb = 0u64;
        let mut free_kb = 0u64;

        for line in content.lines() {
            if line.starts_with("SwapTotal:") {
                total_kb = Self::parse_meminfo_value(line);
            } else if line.starts_with("SwapFree:") {
                free_kb = Self::parse_meminfo_value(line);
            }
        }

        let used_kb = total_kb.saturating_sub(free_kb);
        let percent = if total_kb > 0 {
            (used_kb as f64 / total_kb as f64) * 100.0
        } else {
            0.0
        };

        let status = if percent >= self.thresholds.swap_critical {
            Status::Critical
        } else if percent >= self.thresholds.swap_warn {
            Status::Warn
        } else {
            Status::Ok
        };

        let line = format!(
            "swap:     {:.0}% ({} / {}){}",
            percent,
            Self::format_bytes(used_kb * 1024),
            Self::format_bytes(total_kb * 1024),
            status.marker()
        );

        let issue = if status != Status::Ok {
            Some(format!(
                "swap: actively using swap ({})",
                Self::format_bytes(used_kb * 1024)
            ))
        } else {
            None
        };

        CheckResult { status, line, issue }
    }

    /// Check root disk health
    fn check_disk(&self) -> CheckResult {
        // Use statvfs to get disk information
        let path = std::ffi::CString::new("/").unwrap();

        let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
        let result = unsafe { libc::statvfs(path.as_ptr(), &mut stat) };

        if result != 0 {
            return CheckResult {
                status: Status::Warn,
                line: "disk /:   unknown".to_string(),
                issue: Some("disk: could not read disk stats".to_string()),
            };
        }

        let block_size = stat.f_frsize as u64;
        let total = stat.f_blocks as u64 * block_size;
        let available = stat.f_bavail as u64 * block_size;
        let used = total.saturating_sub(stat.f_bfree as u64 * block_size);

        let percent = if total > 0 {
            (used as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        let status = if percent >= self.thresholds.disk_critical {
            Status::Critical
        } else if percent >= self.thresholds.disk_warn {
            Status::Warn
        } else {
            Status::Ok
        };

        let line = format!(
            "disk /:   {:.0}% ({} / {}){}",
            percent,
            Self::format_bytes(used),
            Self::format_bytes(total),
            status.marker()
        );

        let issue = if status != Status::Ok {
            Some(format!(
                "disk /: {:.0}% used, only {} available",
                percent,
                Self::format_bytes(available)
            ))
        } else {
            None
        };

        CheckResult { status, line, issue }
    }

    /// Parse numeric value from meminfo line (e.g., "MemTotal:       16384000 kB")
    fn parse_meminfo_value(line: &str) -> u64 {
        line.split_whitespace()
            .nth(1)
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    }

    /// Format bytes into human-readable string
    fn format_bytes(bytes: u64) -> String {
        const KB: u64 = 1024;
        const MB: u64 = KB * 1024;
        const GB: u64 = MB * 1024;
        const TB: u64 = GB * 1024;

        if bytes >= TB {
            format!("{:.1}TB", bytes as f64 / TB as f64)
        } else if bytes >= GB {
            format!("{:.1}GB", bytes as f64 / GB as f64)
        } else if bytes >= MB {
            format!("{:.0}MB", bytes as f64 / MB as f64)
        } else if bytes >= KB {
            format!("{:.0}KB", bytes as f64 / KB as f64)
        } else {
            format!("{}B", bytes)
        }
    }
}

impl MetricProvider for HealthProvider {
    fn path(&self) -> &str {
        "health"
    }

    fn collect(&self) -> Result<MetricValue> {
        let cpu = self.check_cpu();
        let memory = self.check_memory();
        let swap = self.check_swap();
        let disk = self.check_disk();

        // Determine overall status (worst of all)
        let overall_status = [cpu.status, memory.status, swap.status, disk.status]
            .into_iter()
            .max()
            .unwrap_or(Status::Ok);

        // Collect issues
        let issues: Vec<String> = [&cpu, &memory, &swap, &disk]
            .iter()
            .filter_map(|c| c.issue.clone())
            .collect();

        // Build output
        let mut output = format!("status: {}\n\n", overall_status.as_str());

        output.push_str(&cpu.line);
        output.push('\n');
        output.push_str(&memory.line);
        output.push('\n');
        output.push_str(&swap.line);
        output.push('\n');
        output.push_str(&disk.line);
        output.push('\n');

        output.push('\n');
        if issues.is_empty() {
            output.push_str("issues: none\n");
        } else {
            output.push_str("issues:\n");
            for issue in issues {
                output.push_str(&format!("  - {}\n", issue));
            }
        }

        Ok(MetricValue::Text(output))
    }
}

// =============================================================================
// HEALTH PLUGIN
// =============================================================================

/// Plugin that provides system health check.
pub struct HealthPlugin {
    proc_path: String,
}

impl HealthPlugin {
    pub fn new() -> Self {
        Self {
            proc_path: "/proc".to_string(),
        }
    }

    pub fn with_proc_path(proc_path: impl Into<String>) -> Self {
        Self {
            proc_path: proc_path.into(),
        }
    }
}

impl Plugin for HealthPlugin {
    fn name(&self) -> &str {
        "health"
    }

    fn description(&self) -> &str {
        "Aggregated system health check"
    }

    fn register(&self, registry: &mut Registry) -> Result<()> {
        let provider = HealthProvider::with_proc_path(self.proc_path.clone());

        registry
            .insert_provider(Arc::new(provider))
            .map_err(|e| anyhow::anyhow!(e))?;

        tracing::info!("Registered health provider");

        Ok(())
    }
}

impl Default for HealthPlugin {
    fn default() -> Self {
        Self::new()
    }
}

/// Type alias for backwards compatibility.
#[deprecated(since = "0.2.0", note = "Use HealthPlugin instead")]
pub type HealthCollector = HealthPlugin;

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_mock_proc() -> TempDir {
        let dir = TempDir::new().unwrap();

        // /proc/loadavg
        let mut f = fs::File::create(dir.path().join("loadavg")).unwrap();
        writeln!(f, "0.50 0.40 0.30 1/234 5678").unwrap();

        // /proc/cpuinfo (2 cores)
        let mut f = fs::File::create(dir.path().join("cpuinfo")).unwrap();
        writeln!(f, "processor\t: 0\nmodel name\t: Test CPU\n").unwrap();
        writeln!(f, "processor\t: 1\nmodel name\t: Test CPU\n").unwrap();

        // /proc/meminfo
        let mut f = fs::File::create(dir.path().join("meminfo")).unwrap();
        writeln!(f, "MemTotal:        8000000 kB").unwrap();
        writeln!(f, "MemFree:         1000000 kB").unwrap();
        writeln!(f, "MemAvailable:    4000000 kB").unwrap();
        writeln!(f, "SwapTotal:       2000000 kB").unwrap();
        writeln!(f, "SwapFree:        2000000 kB").unwrap();

        dir
    }

    #[test]
    fn test_health_provider_ok() {
        let mock_proc = create_mock_proc();
        let provider = HealthProvider::with_proc_path(
            mock_proc.path().to_string_lossy().to_string()
        );

        let result = provider.collect().unwrap();

        if let MetricValue::Text(s) = result {
            assert!(s.contains("status: OK"));
            assert!(s.contains("cpu:"));
            assert!(s.contains("memory:"));
            assert!(s.contains("swap:"));
            assert!(s.contains("issues: none"));
        } else {
            panic!("Expected Text");
        }
    }

    #[test]
    fn test_health_provider_high_load() {
        let dir = TempDir::new().unwrap();

        // High load (4.0 on 2 cores = 200%)
        let mut f = fs::File::create(dir.path().join("loadavg")).unwrap();
        writeln!(f, "4.00 3.50 3.00 1/234 5678").unwrap();

        let mut f = fs::File::create(dir.path().join("cpuinfo")).unwrap();
        writeln!(f, "processor\t: 0\n").unwrap();
        writeln!(f, "processor\t: 1\n").unwrap();

        let mut f = fs::File::create(dir.path().join("meminfo")).unwrap();
        writeln!(f, "MemTotal:        8000000 kB").unwrap();
        writeln!(f, "MemAvailable:    4000000 kB").unwrap();
        writeln!(f, "SwapTotal:       2000000 kB").unwrap();
        writeln!(f, "SwapFree:        2000000 kB").unwrap();

        let provider = HealthProvider::with_proc_path(
            dir.path().to_string_lossy().to_string()
        );

        let result = provider.collect().unwrap();

        if let MetricValue::Text(s) = result {
            assert!(s.contains("status: CRITICAL"));
            assert!(s.contains("[CRITICAL]"));
            assert!(s.contains("issues:"));
            assert!(s.contains("cpu:"));
        } else {
            panic!("Expected Text");
        }
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(HealthProvider::format_bytes(500), "500B");
        assert_eq!(HealthProvider::format_bytes(1024), "1KB");
        assert_eq!(HealthProvider::format_bytes(1536 * 1024), "2MB");
        assert_eq!(HealthProvider::format_bytes(1_500_000_000), "1.4GB");
        assert_eq!(HealthProvider::format_bytes(2_000_000_000_000), "1.8TB");
    }
}
