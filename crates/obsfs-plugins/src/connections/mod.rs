//! # Connections Plugin - Network Connections
//!
//! Provides information about active TCP and UDP connections.
//! Reads from `/proc/net/tcp`, `/proc/net/tcp6`, `/proc/net/udp`, and `/proc/net/udp6`.

use std::collections::HashMap;
use std::fs;
use std::sync::Arc;

use anyhow::Result;
use obsfs_core::{MetricProvider, MetricValue, Plugin, Registry};

// =============================================================================
// TCP STATES
// =============================================================================

fn tcp_state_name(state: &str) -> &'static str {
    match state {
        "01" => "ESTABLISHED",
        "02" => "SYN_SENT",
        "03" => "SYN_RECV",
        "04" => "FIN_WAIT1",
        "05" => "FIN_WAIT2",
        "06" => "TIME_WAIT",
        "07" => "CLOSE",
        "08" => "CLOSE_WAIT",
        "09" => "LAST_ACK",
        "0A" => "LISTEN",
        "0B" => "CLOSING",
        _ => "UNKNOWN",
    }
}

// =============================================================================
// CONNECTION INFO
// =============================================================================

#[derive(Debug, Clone)]
struct Connection {
    local_addr: String,
    local_port: u16,
    remote_addr: String,
    remote_port: u16,
    state: String,
    #[allow(dead_code)]
    inode: u64,
}

// =============================================================================
// CONNECTION READER
// =============================================================================

struct ConnectionReader {
    proc_path: String,
}

impl ConnectionReader {
    fn new(proc_path: &str) -> Self {
        Self {
            proc_path: proc_path.to_string(),
        }
    }

    fn parse_hex_addr(hex: &str) -> String {
        if hex.len() == 8 {
            // IPv4
            let bytes: Vec<u8> = (0..4)
                .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap_or(0))
                .collect();
            format!("{}.{}.{}.{}", bytes[3], bytes[2], bytes[1], bytes[0])
        } else if hex.len() == 32 {
            // IPv6 (simplified)
            "IPv6".to_string()
        } else {
            hex.to_string()
        }
    }

    fn parse_addr_port(addr_port: &str) -> (String, u16) {
        if let Some((addr, port)) = addr_port.split_once(':') {
            let ip = Self::parse_hex_addr(addr);
            let port = u16::from_str_radix(port, 16).unwrap_or(0);
            (ip, port)
        } else {
            (String::new(), 0)
        }
    }

    fn read_tcp(&self) -> Vec<Connection> {
        let mut connections = Vec::new();

        for file in &["net/tcp", "net/tcp6"] {
            let path = format!("{}/{}", self.proc_path, file);
            if let Ok(content) = fs::read_to_string(&path) {
                for line in content.lines().skip(1) {
                    let fields: Vec<&str> = line.split_whitespace().collect();
                    if fields.len() >= 10 {
                        let (local_addr, local_port) = Self::parse_addr_port(fields[1]);
                        let (remote_addr, remote_port) = Self::parse_addr_port(fields[2]);
                        let state = tcp_state_name(fields[3]).to_string();
                        let inode: u64 = fields[9].parse().unwrap_or(0);

                        connections.push(Connection {
                            local_addr,
                            local_port,
                            remote_addr,
                            remote_port,
                            state,
                            inode,
                        });
                    }
                }
            }
        }

        connections
    }

    fn read_udp(&self) -> Vec<Connection> {
        let mut connections = Vec::new();

        for file in &["net/udp", "net/udp6"] {
            let path = format!("{}/{}", self.proc_path, file);
            if let Ok(content) = fs::read_to_string(&path) {
                for line in content.lines().skip(1) {
                    let fields: Vec<&str> = line.split_whitespace().collect();
                    if fields.len() >= 10 {
                        let (local_addr, local_port) = Self::parse_addr_port(fields[1]);
                        let (remote_addr, remote_port) = Self::parse_addr_port(fields[2]);
                        let inode: u64 = fields[9].parse().unwrap_or(0);

                        connections.push(Connection {
                            local_addr,
                            local_port,
                            remote_addr,
                            remote_port,
                            state: "UDP".to_string(),
                            inode,
                        });
                    }
                }
            }
        }

        connections
    }
}

// =============================================================================
// PROVIDERS
// =============================================================================

/// Provider for TCP connections list.
pub struct TcpConnectionsProvider {
    proc_path: String,
}

impl TcpConnectionsProvider {
    pub fn new(proc_path: &str) -> Self {
        Self {
            proc_path: proc_path.to_string(),
        }
    }
}

impl MetricProvider for TcpConnectionsProvider {
    fn path(&self) -> &str {
        "connections/tcp"
    }

    fn collect(&self) -> Result<MetricValue> {
        let reader = ConnectionReader::new(&self.proc_path);
        let connections = reader.read_tcp();

        let mut out = String::new();
        out.push_str("TCP Connections\n");
        out.push_str(&"=".repeat(70));
        out.push_str("\n\n");
        out.push_str(&format!(
            "{:<6} {:<21} {:<21} {:<12}\n",
            "PROTO", "LOCAL", "REMOTE", "STATE"
        ));
        out.push_str(&"-".repeat(70));
        out.push('\n');

        for conn in &connections {
            let local = format!("{}:{}", conn.local_addr, conn.local_port);
            let remote = format!("{}:{}", conn.remote_addr, conn.remote_port);
            out.push_str(&format!(
                "{:<6} {:<21} {:<21} {:<12}\n",
                "tcp", local, remote, conn.state
            ));
        }

        out.push_str(&format!("\nTotal: {} connections\n", connections.len()));

        Ok(MetricValue::Text(out))
    }
}

/// Provider for UDP connections list.
pub struct UdpConnectionsProvider {
    proc_path: String,
}

impl UdpConnectionsProvider {
    pub fn new(proc_path: &str) -> Self {
        Self {
            proc_path: proc_path.to_string(),
        }
    }
}

impl MetricProvider for UdpConnectionsProvider {
    fn path(&self) -> &str {
        "connections/udp"
    }

    fn collect(&self) -> Result<MetricValue> {
        let reader = ConnectionReader::new(&self.proc_path);
        let connections = reader.read_udp();

        let mut out = String::new();
        out.push_str("UDP Connections\n");
        out.push_str(&"=".repeat(70));
        out.push_str("\n\n");
        out.push_str(&format!(
            "{:<6} {:<21} {:<21}\n",
            "PROTO", "LOCAL", "REMOTE"
        ));
        out.push_str(&"-".repeat(70));
        out.push('\n');

        for conn in &connections {
            let local = format!("{}:{}", conn.local_addr, conn.local_port);
            let remote = format!("{}:{}", conn.remote_addr, conn.remote_port);
            out.push_str(&format!("{:<6} {:<21} {:<21}\n", "udp", local, remote));
        }

        out.push_str(&format!("\nTotal: {} connections\n", connections.len()));

        Ok(MetricValue::Text(out))
    }
}

/// Provider for listening ports.
pub struct ListeningProvider {
    proc_path: String,
}

impl ListeningProvider {
    pub fn new(proc_path: &str) -> Self {
        Self {
            proc_path: proc_path.to_string(),
        }
    }
}

impl MetricProvider for ListeningProvider {
    fn path(&self) -> &str {
        "connections/listening"
    }

    fn collect(&self) -> Result<MetricValue> {
        let reader = ConnectionReader::new(&self.proc_path);
        let tcp = reader.read_tcp();
        let udp = reader.read_udp();

        let mut out = String::new();
        out.push_str("Listening Ports\n");
        out.push_str(&"=".repeat(50));
        out.push_str("\n\n");

        out.push_str("TCP:\n");
        let mut tcp_ports: Vec<_> = tcp
            .iter()
            .filter(|c| c.state == "LISTEN")
            .map(|c| c.local_port)
            .collect();
        tcp_ports.sort();
        tcp_ports.dedup();
        for port in &tcp_ports {
            out.push_str(&format!("  :{}\n", port));
        }

        out.push_str("\nUDP:\n");
        let mut udp_ports: Vec<_> = udp
            .iter()
            .filter(|c| c.local_port > 0)
            .map(|c| c.local_port)
            .collect();
        udp_ports.sort();
        udp_ports.dedup();
        for port in &udp_ports {
            out.push_str(&format!("  :{}\n", port));
        }

        out.push_str(&format!(
            "\nTotal: {} TCP, {} UDP\n",
            tcp_ports.len(),
            udp_ports.len()
        ));

        Ok(MetricValue::Text(out))
    }
}

/// Provider for established connections.
pub struct EstablishedProvider {
    proc_path: String,
}

impl EstablishedProvider {
    pub fn new(proc_path: &str) -> Self {
        Self {
            proc_path: proc_path.to_string(),
        }
    }
}

impl MetricProvider for EstablishedProvider {
    fn path(&self) -> &str {
        "connections/established"
    }

    fn collect(&self) -> Result<MetricValue> {
        let reader = ConnectionReader::new(&self.proc_path);
        let connections: Vec<_> = reader
            .read_tcp()
            .into_iter()
            .filter(|c| c.state == "ESTABLISHED")
            .collect();

        let mut out = String::new();
        out.push_str("Established Connections\n");
        out.push_str(&"=".repeat(70));
        out.push_str("\n\n");
        out.push_str(&format!("{:<21} {:<21}\n", "LOCAL", "REMOTE"));
        out.push_str(&"-".repeat(70));
        out.push('\n');

        for conn in &connections {
            let local = format!("{}:{}", conn.local_addr, conn.local_port);
            let remote = format!("{}:{}", conn.remote_addr, conn.remote_port);
            out.push_str(&format!("{:<21} {:<21}\n", local, remote));
        }

        out.push_str(&format!("\nTotal: {} connections\n", connections.len()));

        Ok(MetricValue::Text(out))
    }
}

/// Provider for connection summary.
pub struct ConnectionSummaryProvider {
    proc_path: String,
}

impl ConnectionSummaryProvider {
    pub fn new(proc_path: &str) -> Self {
        Self {
            proc_path: proc_path.to_string(),
        }
    }
}

impl MetricProvider for ConnectionSummaryProvider {
    fn path(&self) -> &str {
        "connections/summary"
    }

    fn collect(&self) -> Result<MetricValue> {
        let reader = ConnectionReader::new(&self.proc_path);
        let tcp = reader.read_tcp();
        let udp = reader.read_udp();

        // Count TCP states
        let mut state_counts: HashMap<String, u32> = HashMap::new();
        for conn in &tcp {
            *state_counts.entry(conn.state.clone()).or_insert(0) += 1;
        }

        let mut out = String::new();
        out.push_str("Network Connections Summary\n");
        out.push_str(&"=".repeat(50));
        out.push_str("\n\n");

        out.push_str("TCP:\n");
        let states = [
            "ESTABLISHED",
            "LISTEN",
            "TIME_WAIT",
            "CLOSE_WAIT",
            "SYN_SENT",
            "SYN_RECV",
            "FIN_WAIT1",
            "FIN_WAIT2",
            "CLOSING",
            "LAST_ACK",
        ];
        for state in states {
            let count = state_counts.get(state).unwrap_or(&0);
            if *count > 0 {
                out.push_str(&format!("  {:<14} {}\n", format!("{}:", state), count));
            }
        }

        out.push_str(&format!("\nUDP:\n  Active:        {}\n", udp.len()));

        // Top listeners
        let mut listeners: Vec<_> = tcp
            .iter()
            .filter(|c| c.state == "LISTEN")
            .map(|c| c.local_port)
            .collect();
        listeners.sort();
        listeners.dedup();

        if !listeners.is_empty() {
            out.push_str("\nTop listeners:\n");
            for port in listeners.iter().take(10) {
                out.push_str(&format!("  :{}\n", port));
            }
            if listeners.len() > 10 {
                out.push_str(&format!("  ... and {} more\n", listeners.len() - 10));
            }
        }

        Ok(MetricValue::Text(out))
    }
}

// =============================================================================
// CONNECTIONS PLUGIN
// =============================================================================

/// Plugin that provides network connection information.
pub struct ConnectionsPlugin {
    proc_path: String,
}

impl ConnectionsPlugin {
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

impl Plugin for ConnectionsPlugin {
    fn name(&self) -> &str {
        "connections"
    }

    fn description(&self) -> &str {
        "Network connections (TCP/UDP) at /obs/connections/"
    }

    fn register(&self, registry: &mut Registry) -> Result<()> {
        registry
            .insert_provider(Arc::new(TcpConnectionsProvider::new(&self.proc_path)))
            .map_err(|e| anyhow::anyhow!(e))?;

        registry
            .insert_provider(Arc::new(UdpConnectionsProvider::new(&self.proc_path)))
            .map_err(|e| anyhow::anyhow!(e))?;

        registry
            .insert_provider(Arc::new(ListeningProvider::new(&self.proc_path)))
            .map_err(|e| anyhow::anyhow!(e))?;

        registry
            .insert_provider(Arc::new(EstablishedProvider::new(&self.proc_path)))
            .map_err(|e| anyhow::anyhow!(e))?;

        registry
            .insert_provider(Arc::new(ConnectionSummaryProvider::new(&self.proc_path)))
            .map_err(|e| anyhow::anyhow!(e))?;

        Ok(())
    }
}

impl Default for ConnectionsPlugin {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_addr() {
        assert_eq!(ConnectionReader::parse_hex_addr("0100007F"), "127.0.0.1");
        assert_eq!(ConnectionReader::parse_hex_addr("00000000"), "0.0.0.0");
    }

    #[test]
    fn test_tcp_state_name() {
        assert_eq!(tcp_state_name("01"), "ESTABLISHED");
        assert_eq!(tcp_state_name("0A"), "LISTEN");
        assert_eq!(tcp_state_name("06"), "TIME_WAIT");
    }

    #[test]
    fn test_plugin_metadata() {
        let plugin = ConnectionsPlugin::new();
        assert_eq!(plugin.name(), "connections");
        assert!(!plugin.description().is_empty());
    }
}
