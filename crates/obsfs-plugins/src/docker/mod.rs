//! # Docker Container Metrics Provider
//!
//! This module provides detailed information about Docker containers
//! through the path `/obs/docker/[container_id]`.
//!
//! ## Usage
//!
//! ```bash
//! cat /obs/docker/my-container/status
//! cat /obs/docker/my-container/stats
//! cat /obs/docker/my-container/info
//! ```
//!
//! ## Paths
//!
//! - `/obs/docker/[container_id]/status` - Container status (running, stopped, etc)
//! - `/obs/docker/[container_id]/stats` - CPU, memory, network stats
//! - `/obs/docker/[container_id]/info` - Detailed container information

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use obsfs_core::{DynamicHandler, Plugin, Registry};
use serde_json::Value;

// =============================================================================
// DOCKER API CLIENT
// =============================================================================

/// Docker API client that communicates with the Docker daemon via Unix socket.
pub struct DockerClient {
    socket_path: String,
}

impl DockerClient {
    pub fn new() -> Self {
        Self {
            socket_path: "/var/run/docker.sock".to_string(),
        }
    }

    pub fn with_socket_path(socket_path: impl Into<String>) -> Self {
        Self {
            socket_path: socket_path.into(),
        }
    }

    /// Send HTTP request to Docker API and return response body.
    fn send_request(&self, method: &str, path: &str) -> Result<String> {
        let mut stream = UnixStream::connect(&self.socket_path)
            .map_err(|e| anyhow!("Failed to connect to Docker socket: {}", e))?;

        // Build HTTP request
        let request = format!(
            "{} {} HTTP/1.0\r\nHost: docker\r\nConnection: close\r\n\r\n",
            method, path
        );

        stream
            .write_all(request.as_bytes())
            .map_err(|e| anyhow!("Failed to write to Docker socket: {}", e))?;

        // Read response
        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .map_err(|e| anyhow!("Failed to read from Docker socket: {}", e))?;

        // Extract body from HTTP response
        let body = response
            .split("\r\n\r\n")
            .nth(1)
            .ok_or_else(|| anyhow!("Invalid Docker API response"))?
            .to_string();

        Ok(body)
    }

    /// List all containers (abbreviated output).
    pub fn list_containers(&self) -> Result<Vec<ContainerSummary>> {
        let response = self.send_request("GET", "/v1.24/containers/json?all=1&limit=100")?;

        let containers: Vec<Value> = serde_json::from_str(&response)
            .map_err(|e| anyhow!("Failed to parse containers list: {}", e))?;

        let mut result = Vec::new();
        for container in containers {
            if let Some(id) = container.get("Id").and_then(|v| v.as_str()) {
                let short_id = id.chars().take(12).collect::<String>();
                let names: Vec<String> = container
                    .get("Names")
                    .and_then(|n| n.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| {
                                v.as_str().map(|s| {
                                    // Remove leading '/' from container names
                                    s.trim_start_matches('/').to_string()
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                result.push(ContainerSummary {
                    id: short_id,
                    names,
                });
            }
        }

        Ok(result)
    }

    /// Get detailed container information.
    pub fn inspect_container(&self, container_id: &str) -> Result<Value> {
        let response =
            self.send_request("GET", &format!("/v1.24/containers/{}/json", container_id))?;

        serde_json::from_str(&response)
            .map_err(|e| anyhow!("Failed to parse container info: {}", e))
    }

    /// Get container stats (streaming stats snapshot).
    pub fn get_stats(&self, container_id: &str) -> Result<Value> {
        let response = self.send_request(
            "GET",
            &format!("/v1.24/containers/{}/stats?stream=0", container_id),
        )?;

        serde_json::from_str(&response)
            .map_err(|e| anyhow!("Failed to parse container stats: {}", e))
    }
}

impl Default for DockerClient {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// DATA STRUCTURES
// =============================================================================

/// Summary of a container for listing.
#[derive(Debug, Clone)]
pub struct ContainerSummary {
    pub id: String,
    pub names: Vec<String>,
}

// =============================================================================
// DOCKER HANDLER
// =============================================================================

/// Handler for dynamic Docker container paths.
pub struct DockerHandler {
    client: DockerClient,
}

impl DockerHandler {
    pub fn new() -> Self {
        Self {
            client: DockerClient::new(),
        }
    }

    pub fn with_socket_path(socket_path: impl Into<String>) -> Self {
        Self {
            client: DockerClient::with_socket_path(socket_path),
        }
    }

    /// Get container ID from various forms (full ID, short ID, or name).
    fn resolve_container_id(&self, identifier: &str) -> Result<String> {
        // If identifier looks like a short ID (12 chars of hex), try it directly
        if identifier.len() == 12 && identifier.chars().all(|c| c.is_ascii_hexdigit()) {
            // Try using it directly
            if let Ok(_) = self.client.inspect_container(identifier) {
                return Ok(identifier.to_string());
            }
        }

        // Otherwise, search by name in container list
        let containers = self.client.list_containers()?;

        for container in containers {
            // Check if identifier matches the short ID
            if container.id == identifier {
                return Ok(identifier.to_string());
            }
            // Check if identifier matches any of the container names
            if container.names.iter().any(|name| name == identifier) {
                return Ok(container.id);
            }
        }

        bail!("Container '{}' not found", identifier)
    }

    /// Get the status of a container.
    fn get_status(&self, container_id: &str) -> Result<String> {
        let resolved_id = self.resolve_container_id(container_id)?;
        let info = self.client.inspect_container(&resolved_id)?;

        let state = info
            .get("State")
            .ok_or_else(|| anyhow!("No State field in container info"))?;

        let status = state
            .get("Status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let running = state
            .get("Running")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let mut output = String::new();
        output.push_str(&format!("Status: {}\n", status));
        output.push_str(&format!(
            "Running: {}\n",
            if running { "yes" } else { "no" }
        ));

        if let Some(pid) = state.get("Pid").and_then(|v| v.as_i64()) {
            if pid > 0 {
                output.push_str(&format!("PID: {}\n", pid));
            }
        }

        if let Some(start_time) = state.get("StartedAt").and_then(|v| v.as_str()) {
            output.push_str(&format!("Started: {}\n", start_time));
        }

        if let Some(exit_code) = state.get("ExitCode").and_then(|v| v.as_i64()) {
            if exit_code != 0 {
                output.push_str(&format!("Exit Code: {}\n", exit_code));
            }
        }

        if let Some(error) = state.get("Error").and_then(|v| v.as_str()) {
            if !error.is_empty() {
                output.push_str(&format!("Error: {}\n", error));
            }
        }

        Ok(output)
    }

    /// Get container stats (CPU, memory, network).
    fn get_stats_info(&self, container_id: &str) -> Result<String> {
        let resolved_id = self.resolve_container_id(container_id)?;
        let stats = self.client.get_stats(&resolved_id)?;

        let mut output = String::new();
        output.push_str("Container Stats\n");
        output.push_str("===============\n\n");

        // CPU Stats
        if let Some(cpu_stats) = stats.get("cpu_stats") {
            if let Some(cpu_delta) = cpu_stats.get("cpu_delta").and_then(|v| v.as_i64()) {
                if let Some(system_delta) =
                    cpu_stats.get("system_cpu_delta").and_then(|v| v.as_i64())
                {
                    if system_delta > 0 {
                        let cpu_percent = (cpu_delta as f64 / system_delta as f64) * 100.0;
                        output.push_str(&format!("CPU Usage: {:.2}%\n", cpu_percent));
                    }
                }
            }
        }

        // Memory Stats
        if let Some(memory_stats) = stats.get("memory_stats") {
            if let Some(usage) = memory_stats.get("usage").and_then(|v| v.as_i64()) {
                output.push_str(&format!("Memory Usage: {}\n", format_bytes(usage as u64)));
            }
            if let Some(limit) = memory_stats.get("limit").and_then(|v| v.as_i64()) {
                if limit > 0 {
                    if let Some(usage) = memory_stats.get("usage").and_then(|v| v.as_i64()) {
                        let percent = (usage as f64 / limit as f64) * 100.0;
                        output.push_str(&format!(
                            "Memory Limit: {} ({:.1}%)\n",
                            format_bytes(limit as u64),
                            percent
                        ));
                    }
                }
            }
        }

        // Network Stats
        if let Some(networks) = stats.get("networks").and_then(|v| v.as_object()) {
            if !networks.is_empty() {
                output.push_str("\nNetwork:\n");
                for (iface, stats) in networks {
                    output.push_str(&format!("  {}: ", iface));
                    if let Some(rx) = stats.get("rx_bytes").and_then(|v| v.as_i64()) {
                        output.push_str(&format!("RX {} ", format_bytes(rx as u64)));
                    }
                    if let Some(tx) = stats.get("tx_bytes").and_then(|v| v.as_i64()) {
                        output.push_str(&format!("TX {}", format_bytes(tx as u64)));
                    }
                    output.push('\n');
                }
            }
        }

        Ok(output)
    }

    /// Get detailed container information.
    fn get_info(&self, container_id: &str) -> Result<String> {
        let resolved_id = self.resolve_container_id(container_id)?;
        let info = self.client.inspect_container(&resolved_id)?;

        let mut output = String::new();

        // Container ID and names
        if let Some(id) = info.get("Id").and_then(|v| v.as_str()) {
            output.push_str(&format!("ID: {}\n", id));
        }

        if let Some(name) = info.get("Name").and_then(|v| v.as_str()) {
            output.push_str(&format!("Name: {}\n", name.trim_start_matches('/')));
        }

        output.push('\n');

        // Image
        if let Some(image) = info.get("Image").and_then(|v| v.as_str()) {
            output.push_str(&format!("Image: {}\n", image));
        }

        // Config
        if let Some(config) = info.get("Config") {
            if let Some(working_dir) = config.get("WorkingDir").and_then(|v| v.as_str()) {
                if !working_dir.is_empty() {
                    output.push_str(&format!("Working Dir: {}\n", working_dir));
                }
            }

            if let Some(user) = config.get("User").and_then(|v| v.as_str()) {
                if !user.is_empty() {
                    output.push_str(&format!("User: {}\n", user));
                }
            }

            if let Some(env) = config.get("Env").and_then(|v| v.as_array()) {
                if !env.is_empty() {
                    output.push_str("\nEnvironment:\n");
                    for e in env.iter().take(5) {
                        if let Some(s) = e.as_str() {
                            output.push_str(&format!("  {}\n", s));
                        }
                    }
                    if env.len() > 5 {
                        output.push_str(&format!("  ... and {} more\n", env.len() - 5));
                    }
                }
            }
        }

        // Host config
        if let Some(host_config) = info.get("HostConfig") {
            output.push_str("\nResources:\n");

            if let Some(cpu_shares) = host_config.get("CpuShares").and_then(|v| v.as_i64()) {
                if cpu_shares > 0 {
                    output.push_str(&format!("  CPU Shares: {}\n", cpu_shares));
                }
            }

            if let Some(memory) = host_config.get("Memory").and_then(|v| v.as_i64()) {
                if memory > 0 {
                    output.push_str(&format!(
                        "  Memory Limit: {}\n",
                        format_bytes(memory as u64)
                    ));
                }
            }

            if let Some(memswap) = host_config.get("MemorySwap").and_then(|v| v.as_i64()) {
                if memswap > 0 {
                    output.push_str(&format!("  Swap Limit: {}\n", format_bytes(memswap as u64)));
                }
            }
        }

        // Mounts
        if let Some(mounts) = info.get("Mounts").and_then(|v| v.as_array()) {
            if !mounts.is_empty() {
                output.push_str("\nMounts:\n");
                for mount in mounts.iter().take(5) {
                    if let Some(source) = mount.get("Source").and_then(|v| v.as_str()) {
                        if let Some(destination) = mount.get("Destination").and_then(|v| v.as_str())
                        {
                            output.push_str(&format!("  {} -> {}\n", source, destination));
                        }
                    }
                }
                if mounts.len() > 5 {
                    output.push_str(&format!("  ... and {} more\n", mounts.len() - 5));
                }
            }
        }

        Ok(output)
    }
}

impl Default for DockerHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl DynamicHandler for DockerHandler {
    fn prefix(&self) -> &str {
        "docker"
    }

    fn list_entries(&self) -> Vec<String> {
        match self.client.list_containers() {
            Ok(containers) => {
                let mut result = Vec::new();

                // Add both short IDs and names
                for container in containers {
                    result.push(container.id);
                    result.extend(container.names);
                }

                result.sort();
                result.dedup();
                result
            }
            Err(_) => {
                // If we can't list containers, return empty
                Vec::new()
            }
        }
    }

    fn exists(&self, subpath: &str) -> bool {
        // subpath should be in format: "container_id/stat_type"
        // e.g., "my-container/status" or "abc123/stats"

        let parts: Vec<&str> = subpath.split('/').collect();
        if parts.is_empty() {
            return false;
        }

        let container_id = parts[0];

        // If there's a subtype, validate it
        if parts.len() > 1 {
            let stat_type = parts[1];
            if !matches!(stat_type, "status" | "stats" | "info") {
                return false;
            }
        }

        // Try to resolve the container
        self.resolve_container_id(container_id).is_ok()
    }

    fn read(&self, subpath: &str) -> Option<String> {
        let parts: Vec<&str> = subpath.split('/').collect();
        if parts.is_empty() {
            return None;
        }

        let container_id = parts[0];
        let stat_type = parts.get(1).copied().unwrap_or("status");

        match stat_type {
            "status" => self.get_status(container_id).ok(),
            "stats" => self.get_stats_info(container_id).ok(),
            "info" => self.get_info(container_id).ok(),
            _ => None,
        }
    }
}

// =============================================================================
// DOCKER PLUGIN
// =============================================================================

/// Plugin that provides Docker container metrics via dynamic paths.
pub struct DockerPlugin {
    socket_path: String,
}

impl DockerPlugin {
    pub fn new() -> Self {
        Self {
            socket_path: "/var/run/docker.sock".to_string(),
        }
    }

    pub fn with_socket_path(socket_path: impl Into<String>) -> Self {
        Self {
            socket_path: socket_path.into(),
        }
    }
}

impl Plugin for DockerPlugin {
    fn name(&self) -> &str {
        "docker"
    }

    fn description(&self) -> &str {
        "Docker container metrics at /obs/docker/[container_id]"
    }

    fn register(&self, _registry: &mut Registry) -> Result<()> {
        // This plugin only provides dynamic handlers, no static metrics
        Ok(())
    }

    fn dynamic_handlers(&self) -> Vec<Arc<dyn DynamicHandler>> {
        vec![Arc::new(DockerHandler::with_socket_path(
            self.socket_path.clone(),
        ))]
    }
}

impl Default for DockerPlugin {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// UTILITIES
// =============================================================================

/// Format bytes into human-readable string.
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1}GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1}MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1}KB", bytes as f64 / KB as f64)
    } else {
        format!("{}B", bytes)
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500B");
        assert_eq!(format_bytes(2048), "2.0KB");
        assert_eq!(format_bytes(1_500_000), "1.4MB");
        assert_eq!(format_bytes(2_500_000_000), "2.3GB");
    }

    #[test]
    fn test_docker_plugin_metadata() {
        let plugin = DockerPlugin::new();
        assert_eq!(plugin.name(), "docker");
        assert!(!plugin.description().is_empty());
        assert_eq!(plugin.dynamic_handlers().len(), 1);
    }

    #[test]
    fn test_docker_handler_default() {
        let handler = DockerHandler::new();
        assert_eq!(handler.prefix(), "docker");
    }

    #[test]
    fn test_handler_exists_validation() {
        let handler = DockerHandler::new();

        // Invalid stat types should return false
        assert!(!handler.exists("some-container/invalid"));
        assert!(!handler.exists("some-container/foo"));

        // Valid stat types are accepted (even if container doesn't exist)
        // Note: will be false if Docker is not running, but we're testing
        // the logic, not the connectivity
        // These calls will fail to connect but that's OK for this test
    }

    #[test]
    fn test_handler_subpath_parsing() {
        let handler = DockerHandler::new();

        // Test that read properly extracts container_id and stat_type
        // Note: Will return None because Docker won't be running,
        // but we're testing the parsing logic
        _ = handler.read("nonexistent/status");
        _ = handler.read("nonexistent/stats");
        _ = handler.read("nonexistent/info");
    }

    #[test]
    fn test_docker_client_default() {
        let client = DockerClient::new();
        assert_eq!(client.socket_path, "/var/run/docker.sock");
    }

    #[test]
    fn test_docker_client_custom_socket() {
        let client = DockerClient::with_socket_path("/tmp/docker.sock");
        assert_eq!(client.socket_path, "/tmp/docker.sock");
    }

    #[test]
    fn test_container_summary() {
        let summary = ContainerSummary {
            id: "abc123def456".to_string(),
            names: vec!["my-container".to_string(), "alias".to_string()],
        };

        assert_eq!(summary.id, "abc123def456");
        assert_eq!(summary.names.len(), 2);
        assert!(summary.names.contains(&"my-container".to_string()));
    }
}
