//! Network interface metrics from /proc/net/dev.
//!
//! Format of /proc/net/dev:
//! Inter-|   Receive                                                |  Transmit
//!  face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
//!    lo: 1234567  12345    0    0    0     0          0         0  1234567   12345    0    0    0     0       0          0

use obsfs_core::{MetricProvider, MetricValue};
use std::fs;

/// Discovers available network interfaces from /proc/net/dev.
pub fn discover_interfaces(proc_path: &str) -> Vec<String> {
    let netdev_path = format!("{}/net/dev", proc_path);
    let contents = match fs::read_to_string(&netdev_path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };

    contents
        .lines()
        .skip(2) // Skip header lines
        .filter_map(|line| {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 2 {
                let name = parts[0].trim();
                // Filter out loopback
                if name != "lo" {
                    return Some(name.to_string());
                }
            }
            None
        })
        .collect()
}

#[derive(Debug)]
struct NetStats {
    rx_bytes: u64,
    rx_packets: u64,
    tx_bytes: u64,
    tx_packets: u64,
}

fn parse_netdev(proc_path: &str, interface: &str) -> anyhow::Result<NetStats> {
    let netdev_path = format!("{}/net/dev", proc_path);
    let contents = fs::read_to_string(&netdev_path)
        .map_err(|e| anyhow::anyhow!("failed to read {}: {}", netdev_path, e))?;

    for line in contents.lines().skip(2) {
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() >= 2 {
            let name = parts[0].trim();
            if name == interface {
                let values: Vec<&str> = parts[1].split_whitespace().collect();
                if values.len() >= 10 {
                    return Ok(NetStats {
                        rx_bytes: values[0].parse().unwrap_or(0),
                        rx_packets: values[1].parse().unwrap_or(0),
                        tx_bytes: values[8].parse().unwrap_or(0),
                        tx_packets: values[9].parse().unwrap_or(0),
                    });
                }
            }
        }
    }

    Err(anyhow::anyhow!("interface {} not found", interface))
}

#[derive(Debug)]
pub struct NetRxBytesProvider {
    proc_path: String,
    interface: String,
    metric_path: String,
}

impl NetRxBytesProvider {
    pub fn new(proc_path: String, interface: &str) -> Self {
        Self {
            proc_path,
            metric_path: format!("system/net/{}/rx_bytes", interface),
            interface: interface.to_string(),
        }
    }
}

impl MetricProvider for NetRxBytesProvider {
    fn path(&self) -> &str {
        &self.metric_path
    }

    fn collect(&self) -> anyhow::Result<MetricValue> {
        let stats = parse_netdev(&self.proc_path, &self.interface)?;
        Ok(MetricValue::Counter(stats.rx_bytes))
    }
}

#[derive(Debug)]
pub struct NetTxBytesProvider {
    proc_path: String,
    interface: String,
    metric_path: String,
}

impl NetTxBytesProvider {
    pub fn new(proc_path: String, interface: &str) -> Self {
        Self {
            proc_path,
            metric_path: format!("system/net/{}/tx_bytes", interface),
            interface: interface.to_string(),
        }
    }
}

impl MetricProvider for NetTxBytesProvider {
    fn path(&self) -> &str {
        &self.metric_path
    }

    fn collect(&self) -> anyhow::Result<MetricValue> {
        let stats = parse_netdev(&self.proc_path, &self.interface)?;
        Ok(MetricValue::Counter(stats.tx_bytes))
    }
}

#[derive(Debug)]
pub struct NetRxPacketsProvider {
    proc_path: String,
    interface: String,
    metric_path: String,
}

impl NetRxPacketsProvider {
    pub fn new(proc_path: String, interface: &str) -> Self {
        Self {
            proc_path,
            metric_path: format!("system/net/{}/rx_packets", interface),
            interface: interface.to_string(),
        }
    }
}

impl MetricProvider for NetRxPacketsProvider {
    fn path(&self) -> &str {
        &self.metric_path
    }

    fn collect(&self) -> anyhow::Result<MetricValue> {
        let stats = parse_netdev(&self.proc_path, &self.interface)?;
        Ok(MetricValue::Counter(stats.rx_packets))
    }
}

#[derive(Debug)]
pub struct NetTxPacketsProvider {
    proc_path: String,
    interface: String,
    metric_path: String,
}

impl NetTxPacketsProvider {
    pub fn new(proc_path: String, interface: &str) -> Self {
        Self {
            proc_path,
            metric_path: format!("system/net/{}/tx_packets", interface),
            interface: interface.to_string(),
        }
    }
}

impl MetricProvider for NetTxPacketsProvider {
    fn path(&self) -> &str {
        &self.metric_path
    }

    fn collect(&self) -> anyhow::Result<MetricValue> {
        let stats = parse_netdev(&self.proc_path, &self.interface)?;
        Ok(MetricValue::Counter(stats.tx_packets))
    }
}
