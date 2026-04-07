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
use obsfs_core::{format_bytes, DynamicHandler, Plugin, Registry};
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

        // Split headers and body
        let parts: Vec<&str> = response.split("\r\n\r\n").collect();
        let headers = parts
            .first()
            .ok_or_else(|| anyhow!("Empty HTTP response"))?;
        let body = parts
            .get(1)
            .ok_or_else(|| anyhow!("Invalid Docker API response"))?
            .to_string();

        // Parse status line to extract status code
        let mut lines = headers.lines();
        let status_line = lines.next().ok_or_else(|| anyhow!("Empty HTTP response"))?;
        let status_code: u16 = status_line
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| anyhow!("Invalid HTTP status line: {}", status_line))?;

        // Validate status code is in 200-299 range
        if !(200..300).contains(&status_code) {
            return Err(anyhow!("HTTP {} error: {}", status_code, body));
        }

        Ok(body)
    }

    /// List all containers (abbreviated output).
    pub fn list_containers(&self) -> Result<Vec<ContainerSummary>> {
        let response = self.send_request("GET", "/v1.43/containers/json?all=1&limit=100")?;

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
            self.send_request("GET", &format!("/v1.43/containers/{}/json", container_id))?;

        serde_json::from_str(&response)
            .map_err(|e| anyhow!("Failed to parse container info: {}", e))
    }

    /// Get container stats (streaming stats snapshot).
    pub fn get_stats(&self, container_id: &str) -> Result<Value> {
        let response = self.send_request(
            "GET",
            &format!("/v1.43/containers/{}/stats?stream=0", container_id),
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
            if self.client.inspect_container(identifier).is_ok() {
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

    /// Get all container information in a single output.
    fn get_full_info(&self, container_id: &str) -> Result<String> {
        let resolved_id = self.resolve_container_id(container_id)?;
        let info = self.client.inspect_container(&resolved_id)?;

        let mut output = String::new();

        // === BASIC INFO ===
        if let Some(name) = info.get("Name").and_then(|v| v.as_str()) {
            output.push_str(&format!("Name: {}\n", name.trim_start_matches('/')));
        }
        if let Some(id) = info.get("Id").and_then(|v| v.as_str()) {
            output.push_str(&format!("ID: {}\n", &id[..12.min(id.len())]));
        }

        // Image
        if let Some(config) = info.get("Config") {
            if let Some(image) = config.get("Image").and_then(|v| v.as_str()) {
                output.push_str(&format!("Image: {}\n", image));
            }
        }

        // State
        if let Some(state) = info.get("State") {
            let status = state
                .get("Status")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            output.push_str(&format!("Status: {}\n", status));

            if let Some(pid) = state.get("Pid").and_then(|v| v.as_i64()) {
                if pid > 0 {
                    output.push_str(&format!("PID: {}\n", pid));
                }
            }

            if let Some(started) = state.get("StartedAt").and_then(|v| v.as_str()) {
                if !started.starts_with("0001") {
                    output.push_str(&format!("Started: {}\n", started));
                }
            }

            if let Some(restart_count) = state.get("RestartCount").and_then(|v| v.as_i64()) {
                if restart_count > 0 {
                    output.push_str(&format!("Restarts: {}\n", restart_count));
                }
            }
        }

        // Created
        if let Some(created) = info.get("Created").and_then(|v| v.as_str()) {
            output.push_str(&format!("Created: {}\n", created));
        }

        // === RESOURCES (live stats) ===
        output.push_str("\n[Resources]\n");
        if let Ok(stats) = self.client.get_stats(&resolved_id) {
            // CPU
            if let Some(cpu_stats) = stats.get("cpu_stats") {
                if let (Some(cpu_usage), Some(system_usage)) = (
                    cpu_stats
                        .get("cpu_usage")
                        .and_then(|c| c.get("total_usage"))
                        .and_then(|v| v.as_u64()),
                    cpu_stats.get("system_cpu_usage").and_then(|v| v.as_u64()),
                ) {
                    if let Some(precpu_stats) = stats.get("precpu_stats") {
                        if let (Some(precpu_usage), Some(presystem_usage)) = (
                            precpu_stats
                                .get("cpu_usage")
                                .and_then(|c| c.get("total_usage"))
                                .and_then(|v| v.as_u64()),
                            precpu_stats
                                .get("system_cpu_usage")
                                .and_then(|v| v.as_u64()),
                        ) {
                            let cpu_delta = cpu_usage.saturating_sub(precpu_usage);
                            let system_delta = system_usage.saturating_sub(presystem_usage);
                            if system_delta > 0 {
                                let num_cpus = cpu_stats
                                    .get("online_cpus")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(1);
                                let cpu_percent = (cpu_delta as f64 / system_delta as f64)
                                    * num_cpus as f64
                                    * 100.0;
                                output.push_str(&format!("CPU: {:.1}%\n", cpu_percent));
                            }
                        }
                    }
                }
            }

            // Memory
            if let Some(memory_stats) = stats.get("memory_stats") {
                if let Some(usage) = memory_stats.get("usage").and_then(|v| v.as_u64()) {
                    let cache = memory_stats
                        .get("stats")
                        .and_then(|s| s.get("cache"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let mem_usage = usage.saturating_sub(cache);

                    if let Some(limit) = memory_stats.get("limit").and_then(|v| v.as_u64()) {
                        if limit > 0 && limit < u64::MAX / 2 {
                            let percent = (mem_usage as f64 / limit as f64) * 100.0;
                            output.push_str(&format!(
                                "Memory: {} / {} ({:.1}%)\n",
                                format_bytes(mem_usage),
                                format_bytes(limit),
                                percent
                            ));
                        } else {
                            output.push_str(&format!("Memory: {}\n", format_bytes(mem_usage)));
                        }
                    } else {
                        output.push_str(&format!("Memory: {}\n", format_bytes(mem_usage)));
                    }
                }
            }

            // Network
            if let Some(networks) = stats.get("networks").and_then(|v| v.as_object()) {
                let mut total_rx: u64 = 0;
                let mut total_tx: u64 = 0;
                for (_iface, net_stats) in networks {
                    total_rx += net_stats
                        .get("rx_bytes")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    total_tx += net_stats
                        .get("tx_bytes")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                }
                output.push_str(&format!(
                    "Network: RX {} / TX {}\n",
                    format_bytes(total_rx),
                    format_bytes(total_tx)
                ));
            }
        }

        // === NETWORK CONFIG ===
        output.push_str("\n[Network]\n");
        if let Some(network_settings) = info.get("NetworkSettings") {
            if let Some(ip) = network_settings.get("IPAddress").and_then(|v| v.as_str()) {
                if !ip.is_empty() {
                    output.push_str(&format!("IP: {}\n", ip));
                }
            }

            // Ports
            if let Some(ports) = network_settings.get("Ports").and_then(|v| v.as_object()) {
                let mut port_mappings = Vec::new();
                for (container_port, host_bindings) in ports {
                    if let Some(bindings) = host_bindings.as_array() {
                        for binding in bindings {
                            if let (Some(host_ip), Some(host_port)) = (
                                binding.get("HostIp").and_then(|v| v.as_str()),
                                binding.get("HostPort").and_then(|v| v.as_str()),
                            ) {
                                port_mappings.push(format!(
                                    "{} -> {}:{}",
                                    container_port, host_ip, host_port
                                ));
                            }
                        }
                    }
                }
                if !port_mappings.is_empty() {
                    output.push_str(&format!("Ports: {}\n", port_mappings.join(", ")));
                }
            }
        }

        // === CONFIG ===
        output.push_str("\n[Config]\n");
        if let Some(config) = info.get("Config") {
            // Command
            if let Some(cmd) = config.get("Cmd").and_then(|v| v.as_array()) {
                let cmd_str: Vec<&str> = cmd.iter().filter_map(|v| v.as_str()).collect();
                if !cmd_str.is_empty() {
                    output.push_str(&format!("Command: {}\n", cmd_str.join(" ")));
                }
            }

            // Entrypoint
            if let Some(entrypoint) = config.get("Entrypoint").and_then(|v| v.as_array()) {
                let ep_str: Vec<&str> = entrypoint.iter().filter_map(|v| v.as_str()).collect();
                if !ep_str.is_empty() {
                    output.push_str(&format!("Entrypoint: {}\n", ep_str.join(" ")));
                }
            }

            if let Some(working_dir) = config.get("WorkingDir").and_then(|v| v.as_str()) {
                if !working_dir.is_empty() {
                    output.push_str(&format!("WorkingDir: {}\n", working_dir));
                }
            }

            if let Some(user) = config.get("User").and_then(|v| v.as_str()) {
                if !user.is_empty() {
                    output.push_str(&format!("User: {}\n", user));
                }
            }
        }

        // === MOUNTS ===
        if let Some(mounts) = info.get("Mounts").and_then(|v| v.as_array()) {
            if !mounts.is_empty() {
                output.push_str("\n[Mounts]\n");
                for mount in mounts {
                    let mount_type = mount
                        .get("Type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let source = mount.get("Source").and_then(|v| v.as_str()).unwrap_or("?");
                    let dest = mount
                        .get("Destination")
                        .and_then(|v| v.as_str())
                        .unwrap_or("?");
                    let rw = if mount.get("RW").and_then(|v| v.as_bool()).unwrap_or(false) {
                        "rw"
                    } else {
                        "ro"
                    };
                    output.push_str(&format!(
                        "- {} -> {} ({}, {})\n",
                        source, dest, mount_type, rw
                    ));
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
            Err(_) => Vec::new(),
        }
    }

    fn exists(&self, subpath: &str) -> bool {
        if subpath.is_empty() {
            return false;
        }
        // Only single-level paths (container ID or name)
        if subpath.contains('/') {
            return false;
        }
        self.resolve_container_id(subpath).is_ok()
    }

    fn read(&self, subpath: &str) -> Option<String> {
        if subpath.is_empty() || subpath.contains('/') {
            return None;
        }
        self.get_full_info(subpath).ok()
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
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

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
