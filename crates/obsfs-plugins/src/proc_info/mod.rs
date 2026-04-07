//! # Process Info Provider - Per-process Information
//!
//! Provides detailed information about any process via dynamic paths at `/obs/proc/[pid]`.
//! Includes CPU time, memory usage, threads, file descriptors, and network connections.

use std::fs;
use std::sync::Arc;

use anyhow::Result;
use obsfs_core::{DynamicHandler, Plugin, Registry};

// =============================================================================
// PROCESS INFO
// =============================================================================

/// Collected information about a process from various /proc sources.
#[derive(Debug, Default)]
struct ProcessInfo {
    pid: u32,
    name: String,
    state: String,
    state_desc: String,
    ppid: u32,
    uid: u32,
    username: String,

    // CPU
    utime: u64, // user time in ticks
    stime: u64, // system time in ticks

    // Memory
    rss_bytes: u64,   // Resident Set Size
    vsize_bytes: u64, // Virtual Size

    // Threads and FDs
    threads: u32,
    fd_count: u32,
    fd_limit: u32,

    // Timing
    start_time: u64,  // time since boot in ticks
    uptime_secs: u64, // system uptime

    // Command
    cmdline: String,
    cwd: String,

    // Network
    tcp_established: u32,
    tcp_listen: u32,
    listen_ports: Vec<u16>,
}

// =============================================================================
// PROCESS INFO PROVIDER
// =============================================================================

/// Provides detailed information about a specific process.
pub struct ProcessInfoProvider {
    proc_path: String,
}

impl ProcessInfoProvider {
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

    /// Collect all information about a process by PID.
    pub fn collect_for_pid(&self, pid: u32) -> Result<String> {
        let proc_dir = format!("{}/{}", self.proc_path, pid);

        // Check if process exists
        if !std::path::Path::new(&proc_dir).exists() {
            anyhow::bail!("Process {} not found", pid);
        }

        let mut info = ProcessInfo {
            pid,
            ..Default::default()
        };

        // Collect information from various sources
        self.read_comm(&proc_dir, &mut info);
        self.read_status(&proc_dir, &mut info);
        self.read_stat(&proc_dir, &mut info);
        self.read_cmdline(&proc_dir, &mut info);
        self.read_cwd(&proc_dir, &mut info);
        self.read_fd_count(&proc_dir, &mut info);
        self.read_limits(&proc_dir, &mut info);
        self.read_uptime(&mut info);
        self.read_network_info(&proc_dir, &mut info);

        // Format the output
        Ok(self.format_output(&info))
    }

    /// Read process name from /proc/[pid]/comm
    fn read_comm(&self, proc_dir: &str, info: &mut ProcessInfo) {
        if let Ok(comm) = fs::read_to_string(format!("{}/comm", proc_dir)) {
            info.name = comm.trim().to_string();
        }
    }

    /// Read information from /proc/[pid]/status
    fn read_status(&self, proc_dir: &str, info: &mut ProcessInfo) {
        if let Ok(content) = fs::read_to_string(format!("{}/status", proc_dir)) {
            for line in content.lines() {
                if line.starts_with("State:") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        info.state = parts[1].to_string();
                        info.state_desc = match info.state.as_str() {
                            "R" => "Running",
                            "S" => "Sleeping",
                            "D" => "Waiting (I/O)",
                            "Z" => "Zombie",
                            "T" => "Stopped",
                            "t" => "Tracing stop",
                            "X" => "Dead",
                            _ => "Unknown",
                        }
                        .to_string();
                    }
                } else if line.starts_with("PPid:") {
                    info.ppid = Self::parse_status_value(line);
                } else if line.starts_with("Uid:") {
                    // Format: Uid: real effective saved fs
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        info.uid = parts[1].parse().unwrap_or(0);
                    }
                } else if line.starts_with("Threads:") {
                    info.threads = Self::parse_status_value::<u32>(line);
                } else if line.starts_with("VmRSS:") {
                    let kb: u64 = Self::parse_status_value::<u64>(line);
                    info.rss_bytes = kb * 1024;
                } else if line.starts_with("VmSize:") {
                    let kb: u64 = Self::parse_status_value::<u64>(line);
                    info.vsize_bytes = kb * 1024;
                }
            }
        }

        // Resolve username from UID
        info.username = self.uid_to_username(info.uid);
    }

    /// Read information from /proc/[pid]/stat
    fn read_stat(&self, proc_dir: &str, info: &mut ProcessInfo) {
        if let Ok(content) = fs::read_to_string(format!("{}/stat", proc_dir)) {
            // Format: pid (comm) state ppid pgrp session tty_nr tpgid flags
            //         minflt cminflt majflt cmajflt utime stime cutime cstime
            //         priority nice num_threads itrealvalue starttime ...

            // Find the closing parenthesis of comm (which may contain spaces)
            if let Some(comm_end) = content.rfind(')') {
                let after_comm = &content[comm_end + 2..]; // Skip ") "
                let fields: Vec<&str> = after_comm.split_whitespace().collect();

                // utime is field 11 (0-indexed), stime is field 12
                // starttime is field 19
                if fields.len() > 19 {
                    info.utime = fields[11].parse().unwrap_or(0);
                    info.stime = fields[12].parse().unwrap_or(0);
                    info.start_time = fields[19].parse().unwrap_or(0);
                }
            }
        }
    }

    /// Read full command line from /proc/[pid]/cmdline
    fn read_cmdline(&self, proc_dir: &str, info: &mut ProcessInfo) {
        if let Ok(content) = fs::read_to_string(format!("{}/cmdline", proc_dir)) {
            // cmdline uses \0 as separator
            info.cmdline = content.replace('\0', " ").trim().to_string();
            if info.cmdline.is_empty() {
                info.cmdline = format!("[{}]", info.name);
            }
        }
    }

    /// Read working directory from /proc/[pid]/cwd
    fn read_cwd(&self, proc_dir: &str, info: &mut ProcessInfo) {
        if let Ok(cwd) = fs::read_link(format!("{}/cwd", proc_dir)) {
            info.cwd = cwd.to_string_lossy().to_string();
        }
    }

    /// Count file descriptors in /proc/[pid]/fd
    fn read_fd_count(&self, proc_dir: &str, info: &mut ProcessInfo) {
        if let Ok(entries) = fs::read_dir(format!("{}/fd", proc_dir)) {
            info.fd_count = entries.count() as u32;
        }
    }

    /// Read limits from /proc/[pid]/limits
    fn read_limits(&self, proc_dir: &str, info: &mut ProcessInfo) {
        if let Ok(content) = fs::read_to_string(format!("{}/limits", proc_dir)) {
            for line in content.lines() {
                if line.starts_with("Max open files") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    // Format: Max open files            1024                 1024                 files
                    if parts.len() >= 5 {
                        info.fd_limit = parts[3].parse().unwrap_or(1024);
                    }
                }
            }
        }
    }

    /// Read system uptime
    fn read_uptime(&self, info: &mut ProcessInfo) {
        if let Ok(content) = fs::read_to_string(format!("{}/uptime", self.proc_path)) {
            if let Some(uptime_str) = content.split_whitespace().next() {
                info.uptime_secs = uptime_str.parse::<f64>().unwrap_or(0.0) as u64;
            }
        }
    }

    /// Read network information for the process
    fn read_network_info(&self, proc_dir: &str, info: &mut ProcessInfo) {
        // Read socket inodes for this process
        let mut socket_inodes: Vec<u64> = Vec::new();

        if let Ok(entries) = fs::read_dir(format!("{}/fd", proc_dir)) {
            for entry in entries.flatten() {
                if let Ok(link) = fs::read_link(entry.path()) {
                    let link_str = link.to_string_lossy();
                    // socket:[12345]
                    if link_str.starts_with("socket:[") {
                        if let Some(inode_str) = link_str
                            .strip_prefix("socket:[")
                            .and_then(|s| s.strip_suffix(']'))
                        {
                            if let Ok(inode) = inode_str.parse() {
                                socket_inodes.push(inode);
                            }
                        }
                    }
                }
            }
        }

        // Read /proc/net/tcp to find connections for this process
        if let Ok(content) = fs::read_to_string(format!("{}/net/tcp", self.proc_path)) {
            for line in content.lines().skip(1) {
                // Skip header
                let fields: Vec<&str> = line.split_whitespace().collect();
                if fields.len() >= 10 {
                    let inode: u64 = fields[9].parse().unwrap_or(0);

                    if socket_inodes.contains(&inode) {
                        // st (state): 01=ESTABLISHED, 0A=LISTEN
                        let state = fields[3];

                        match state {
                            "01" => info.tcp_established += 1,
                            "0A" => {
                                info.tcp_listen += 1;
                                // Parse local port
                                if let Some(port_hex) = fields[1].split(':').nth(1) {
                                    if let Ok(port) = u16::from_str_radix(port_hex, 16) {
                                        info.listen_ports.push(port);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    /// Convert UID to username by reading /etc/passwd
    fn uid_to_username(&self, uid: u32) -> String {
        // Try to read /etc/passwd
        if let Ok(content) = fs::read_to_string("/etc/passwd") {
            for line in content.lines() {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() >= 3 {
                    if let Ok(line_uid) = parts[2].parse::<u32>() {
                        if line_uid == uid {
                            return parts[0].to_string();
                        }
                    }
                }
            }
        }
        format!("uid:{}", uid)
    }

    /// Parse numeric value from status file line
    fn parse_status_value<T: std::str::FromStr + Default>(line: &str) -> T {
        line.split_whitespace()
            .nth(1)
            .and_then(|v| v.parse().ok())
            .unwrap_or_default()
    }

    /// Format the final output string
    fn format_output(&self, info: &ProcessInfo) -> String {
        let mut out = String::new();

        // Header
        out.push_str(&format!("Process: {} (PID {})\n", info.name, info.pid));
        out.push_str(&"=".repeat(50));
        out.push_str("\n\n");

        // Status
        out.push_str(&format!(
            "Status:      {} ({})\n",
            info.state_desc, info.state
        ));
        out.push_str(&format!(
            "User:        {} (uid {})\n",
            info.username, info.uid
        ));
        out.push_str(&format!("Parent:      PID {}\n", info.ppid));
        out.push_str(&format!(
            "Uptime:      {}\n",
            self.format_process_uptime(info)
        ));
        out.push('\n');

        // Resources
        out.push_str(&format!(
            "CPU Time:    {} user, {} sys\n",
            Self::format_ticks(info.utime),
            Self::format_ticks(info.stime)
        ));
        out.push_str(&format!(
            "Memory:      {} RSS / {} Virtual\n",
            Self::format_bytes(info.rss_bytes),
            Self::format_bytes(info.vsize_bytes)
        ));
        out.push_str(&format!("Threads:     {}\n", info.threads));
        out.push('\n');

        // File descriptors
        let fd_percent = if info.fd_limit > 0 {
            (info.fd_count as f64 / info.fd_limit as f64 * 100.0) as u32
        } else {
            0
        };
        out.push_str(&format!(
            "FDs:         {} / {} ({}%)\n",
            info.fd_count, info.fd_limit, fd_percent
        ));

        // Network
        if info.tcp_established > 0 || info.tcp_listen > 0 {
            out.push('\n');
            out.push_str("Network:\n");
            if info.tcp_established > 0 {
                out.push_str(&format!("  TCP established: {}\n", info.tcp_established));
            }
            if info.tcp_listen > 0 {
                let ports: Vec<String> = info.listen_ports.iter().map(|p| p.to_string()).collect();
                out.push_str(&format!(
                    "  TCP listening:   {} (ports: {})\n",
                    info.tcp_listen,
                    ports.join(", ")
                ));
            }
        }

        // Paths
        out.push('\n');
        if !info.cwd.is_empty() {
            out.push_str(&format!("Cwd:         {}\n", info.cwd));
        }
        out.push_str(&format!("Command:     {}\n", info.cmdline));

        out
    }

    /// Format process uptime
    fn format_process_uptime(&self, info: &ProcessInfo) -> String {
        // start_time is in clock ticks since boot
        // Convert to seconds
        let ticks_per_sec = 100u64; // typically 100 Hz (CONFIG_HZ)

        let start_secs = info.start_time / ticks_per_sec;
        let running_secs = info.uptime_secs.saturating_sub(start_secs);

        Self::format_duration(running_secs)
    }

    /// Format duration in human-readable format
    fn format_duration(secs: u64) -> String {
        let days = secs / 86400;
        let hours = (secs % 86400) / 3600;
        let mins = (secs % 3600) / 60;

        if days > 0 {
            format!("{}d {}h {}m", days, hours, mins)
        } else if hours > 0 {
            format!("{}h {}m", hours, mins)
        } else {
            format!("{}m", mins)
        }
    }

    /// Format CPU ticks in human-readable format
    fn format_ticks(ticks: u64) -> String {
        let ticks_per_sec = 100u64;
        let secs = ticks / ticks_per_sec;
        Self::format_duration(secs)
    }

    /// Format bytes in human-readable format
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

    /// List all active PIDs for dynamic directory listing
    pub fn list_pids(&self) -> Vec<u32> {
        let mut pids = Vec::new();

        if let Ok(entries) = fs::read_dir(&self.proc_path) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if let Ok(pid) = name.parse::<u32>() {
                        pids.push(pid);
                    }
                }
            }
        }

        pids.sort();
        pids
    }
}

impl Default for ProcessInfoProvider {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// DYNAMIC HANDLER IMPLEMENTATION
// =============================================================================

impl DynamicHandler for ProcessInfoProvider {
    fn prefix(&self) -> &str {
        "proc"
    }

    fn list_entries(&self) -> Vec<String> {
        self.list_pids()
            .into_iter()
            .map(|pid| pid.to_string())
            .collect()
    }

    fn exists(&self, subpath: &str) -> bool {
        if let Ok(pid) = subpath.parse::<u32>() {
            let proc_dir = format!("{}/{}", self.proc_path, pid);
            std::path::Path::new(&proc_dir).exists()
        } else {
            false
        }
    }

    fn read(&self, subpath: &str) -> Option<String> {
        let pid: u32 = subpath.parse().ok()?;
        self.collect_for_pid(pid).ok()
    }
}

// =============================================================================
// PROCESS INFO PLUGIN
// =============================================================================

/// Plugin that provides per-process information via dynamic paths.
pub struct ProcessInfoPlugin {
    proc_path: String,
}

impl ProcessInfoPlugin {
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

impl Plugin for ProcessInfoPlugin {
    fn name(&self) -> &str {
        "proc-info"
    }

    fn description(&self) -> &str {
        "Per-process information at /obs/proc/[pid]"
    }

    fn register(&self, _registry: &mut Registry) -> Result<()> {
        // This plugin only provides dynamic handlers, no static metrics
        Ok(())
    }

    fn dynamic_handlers(&self) -> Vec<Arc<dyn DynamicHandler>> {
        vec![Arc::new(ProcessInfoProvider::with_proc_path(
            self.proc_path.clone(),
        ))]
    }
}

impl Default for ProcessInfoPlugin {
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
        assert_eq!(ProcessInfoProvider::format_bytes(500), "500B");
        assert_eq!(ProcessInfoProvider::format_bytes(2048), "2KB");
        assert_eq!(ProcessInfoProvider::format_bytes(1_500_000), "1.4MB");
        assert_eq!(ProcessInfoProvider::format_bytes(2_500_000_000), "2.3GB");
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(ProcessInfoProvider::format_duration(30), "0m");
        assert_eq!(ProcessInfoProvider::format_duration(90), "1m");
        assert_eq!(ProcessInfoProvider::format_duration(3700), "1h 1m");
        assert_eq!(ProcessInfoProvider::format_duration(90061), "1d 1h 1m");
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_list_pids() {
        let provider = ProcessInfoProvider::new();
        let pids = provider.list_pids();

        // Should find at least the current process
        assert!(!pids.is_empty());
        assert!(pids.contains(&std::process::id()));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_collect_for_current_process() {
        let provider = ProcessInfoProvider::new();
        let pid = std::process::id();

        let result = provider.collect_for_pid(pid);
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.contains(&format!("PID {}", pid)));
        assert!(output.contains("Status:"));
        assert!(output.contains("Memory:"));
    }
}
