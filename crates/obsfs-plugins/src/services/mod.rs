//! # Services Plugin - Systemd Service Status
//!
//! Provides status information about systemd services via dynamic paths.
//! Shows service status, PID, memory usage, and description for each service.

use std::process::Command;
use std::sync::Arc;

use anyhow::Result;
use obsfs_core::{DynamicHandler, Plugin, Registry};

// =============================================================================
// SERVICE INFO
// =============================================================================

#[derive(Debug, Default)]
struct ServiceInfo {
    name: String,
    load_state: String,
    active_state: String,
    sub_state: String,
    description: String,
    main_pid: u32,
    memory_bytes: u64,
    tasks: u32,
    started_at: String,
}

// =============================================================================
// SERVICE INFO PROVIDER
// =============================================================================

/// Provides detailed information about systemd services.
pub struct ServiceInfoProvider;

impl ServiceInfoProvider {
    pub fn new() -> Self {
        Self
    }

    /// List all available services from systemctl.
    pub fn list_services(&self) -> Vec<String> {
        let output = Command::new("systemctl")
            .args([
                "list-units",
                "--type=service",
                "--all",
                "--no-legend",
                "--no-pager",
            ])
            .output();

        match output {
            Ok(out) if out.status.success() => {
                String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .filter_map(|line| {
                        let name = line.split_whitespace().next()?;
                        // Remove .service suffix for cleaner paths
                        Some(name.trim_end_matches(".service").to_string())
                    })
                    .collect()
            }
            _ => vec![],
        }
    }

    /// Collect information for a specific service.
    pub fn collect_for_service(&self, name: &str) -> Result<String> {
        let service_name = if name.ends_with(".service") {
            name.to_string()
        } else {
            format!("{}.service", name)
        };

        let output = Command::new("systemctl")
            .args(["show", &service_name, "--no-pager"])
            .output()?;

        if !output.status.success() {
            anyhow::bail!("Service '{}' not found", name);
        }

        let content = String::from_utf8_lossy(&output.stdout);
        let mut info = ServiceInfo {
            name: service_name.clone(),
            ..Default::default()
        };

        for line in content.lines() {
            if let Some((key, value)) = line.split_once('=') {
                match key {
                    "LoadState" => info.load_state = value.to_string(),
                    "ActiveState" => info.active_state = value.to_string(),
                    "SubState" => info.sub_state = value.to_string(),
                    "Description" => info.description = value.to_string(),
                    "MainPID" => info.main_pid = value.parse().unwrap_or(0),
                    "MemoryCurrent" => {
                        info.memory_bytes = value.parse().unwrap_or(0);
                    }
                    "TasksCurrent" => info.tasks = value.parse().unwrap_or(0),
                    "ActiveEnterTimestamp" => info.started_at = value.to_string(),
                    _ => {}
                }
            }
        }

        Ok(self.format_output(&info))
    }

    fn format_output(&self, info: &ServiceInfo) -> String {
        let mut out = String::new();

        out.push_str(&format!("Service: {}\n", info.name));
        out.push_str(&"=".repeat(50));
        out.push_str("\n\n");

        // Status
        let status = if info.active_state == "active" {
            format!("{} ({})", info.active_state, info.sub_state)
        } else {
            info.active_state.clone()
        };
        out.push_str(&format!("Status:      {}\n", status));
        out.push_str(&format!("Load:        {}\n", info.load_state));

        if info.main_pid > 0 {
            out.push_str(&format!("PID:         {}\n", info.main_pid));
        }

        if info.memory_bytes > 0 {
            out.push_str(&format!(
                "Memory:      {}\n",
                Self::format_bytes(info.memory_bytes)
            ));
        }

        if info.tasks > 0 {
            out.push_str(&format!("Tasks:       {}\n", info.tasks));
        }

        if !info.started_at.is_empty() && info.started_at != "n/a" {
            out.push_str(&format!("Started:     {}\n", info.started_at));
        }

        out.push('\n');
        out.push_str(&format!("Description: {}\n", info.description));

        out
    }

    fn format_bytes(bytes: u64) -> String {
        const KB: u64 = 1024;
        const MB: u64 = KB * 1024;
        const GB: u64 = MB * 1024;

        if bytes >= GB {
            format!("{:.1}GB", bytes as f64 / GB as f64)
        } else if bytes >= MB {
            format!("{:.1}MB", bytes as f64 / MB as f64)
        } else if bytes >= KB {
            format!("{}KB", bytes / KB)
        } else {
            format!("{}B", bytes)
        }
    }
}

impl Default for ServiceInfoProvider {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// DYNAMIC HANDLER
// =============================================================================

impl DynamicHandler for ServiceInfoProvider {
    fn prefix(&self) -> &str {
        "services"
    }

    fn list_entries(&self) -> Vec<String> {
        self.list_services()
    }

    fn exists(&self, subpath: &str) -> bool {
        // subpath can be "nginx" or "nginx/status"
        let service_name = subpath.split('/').next().unwrap_or(subpath);

        let output = Command::new("systemctl")
            .args([
                "show",
                &format!("{}.service", service_name),
                "--property=LoadState",
            ])
            .output();

        match output {
            Ok(out) => {
                let content = String::from_utf8_lossy(&out.stdout);
                !content.contains("not-found")
            }
            Err(_) => false,
        }
    }

    fn read(&self, subpath: &str) -> Option<String> {
        // Handle both "nginx" and "nginx/status"
        let service_name = subpath.split('/').next()?;
        self.collect_for_service(service_name).ok()
    }
}

// =============================================================================
// SERVICES PLUGIN
// =============================================================================

/// Plugin that provides systemd service status.
pub struct ServicesPlugin;

impl ServicesPlugin {
    pub fn new() -> Self {
        Self
    }
}

impl Plugin for ServicesPlugin {
    fn name(&self) -> &str {
        "services"
    }

    fn description(&self) -> &str {
        "Systemd service status at /obs/services/[name]"
    }

    fn register(&self, _registry: &mut Registry) -> Result<()> {
        Ok(())
    }

    fn dynamic_handlers(&self) -> Vec<Arc<dyn DynamicHandler>> {
        vec![Arc::new(ServiceInfoProvider::new())]
    }
}

impl Default for ServicesPlugin {
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
    fn test_format_bytes() {
        assert_eq!(ServiceInfoProvider::format_bytes(500), "500B");
        assert_eq!(ServiceInfoProvider::format_bytes(2048), "2KB");
        assert_eq!(ServiceInfoProvider::format_bytes(1_500_000), "1.4MB");
        assert_eq!(ServiceInfoProvider::format_bytes(2_500_000_000), "2.3GB");
    }

    #[test]
    fn test_plugin_metadata() {
        let plugin = ServicesPlugin::new();
        assert_eq!(plugin.name(), "services");
        assert!(!plugin.description().is_empty());
        assert_eq!(plugin.dynamic_handlers().len(), 1);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_list_services() {
        let provider = ServiceInfoProvider::new();
        let services = provider.list_services();
        // On a systemd system, there should be at least some services
        // This test may fail on non-systemd systems
        println!("Found {} services", services.len());
    }
}
