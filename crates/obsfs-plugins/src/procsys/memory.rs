//! Memory metrics from /proc/meminfo.

use std::collections::HashMap;
use std::fs;
use obsfs_core::{MetricProvider, MetricValue};

/// Provides a single memory metric from /proc/meminfo.
#[derive(Debug)]
pub struct MemoryMetricProvider {
    proc_path: String,
    metric_path: String,
    meminfo_field: String,
}

impl MemoryMetricProvider {
    pub fn new(proc_path: String, metric_path: &str, meminfo_field: &str) -> Self {
        Self {
            proc_path,
            metric_path: metric_path.to_string(),
            meminfo_field: meminfo_field.to_string(),
        }
    }

    fn parse_meminfo(&self) -> anyhow::Result<HashMap<String, u64>> {
        let meminfo_path = format!("{}/meminfo", self.proc_path);
        let contents = fs::read_to_string(&meminfo_path)
            .map_err(|e| anyhow::anyhow!("failed to read {}: {}", meminfo_path, e))?;

        let mut result = HashMap::new();

        for line in contents.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let field = parts[0].trim_end_matches(':');
                if let Ok(value) = parts[1].parse::<u64>() {
                    result.insert(field.to_string(), value * 1024);
                }
            }
        }

        Ok(result)
    }
}

impl MetricProvider for MemoryMetricProvider {
    fn path(&self) -> &str {
        &self.metric_path
    }

    fn collect(&self) -> anyhow::Result<MetricValue> {
        let meminfo = self.parse_meminfo()?;

        let value = meminfo
            .get(&self.meminfo_field)
            .ok_or_else(|| anyhow::anyhow!("field {} not found in meminfo", self.meminfo_field))?;

        Ok(MetricValue::Counter(*value))
    }
}

/// Provides swap used (calculated as SwapTotal - SwapFree).
#[derive(Debug)]
pub struct SwapUsedProvider {
    proc_path: String,
}

impl SwapUsedProvider {
    pub fn new(proc_path: String) -> Self {
        Self { proc_path }
    }
}

impl MetricProvider for SwapUsedProvider {
    fn path(&self) -> &str {
        "system/memory/swap_used"
    }

    fn collect(&self) -> anyhow::Result<MetricValue> {
        let meminfo_path = format!("{}/meminfo", self.proc_path);
        let contents = fs::read_to_string(&meminfo_path)?;

        let mut swap_total: u64 = 0;
        let mut swap_free: u64 = 0;

        for line in contents.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let field = parts[0].trim_end_matches(':');
                if let Ok(value) = parts[1].parse::<u64>() {
                    match field {
                        "SwapTotal" => swap_total = value * 1024,
                        "SwapFree" => swap_free = value * 1024,
                        _ => {}
                    }
                }
            }
        }

        Ok(MetricValue::Counter(swap_total.saturating_sub(swap_free)))
    }
}
