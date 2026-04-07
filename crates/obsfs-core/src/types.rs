//! Core type definitions for ObsFS.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

/// Represents the value returned by a metric collector.
#[derive(Debug)]
pub enum MetricValue {
    /// A point-in-time numeric measurement (e.g., CPU usage percentage).
    Gauge(f64),

    /// A monotonically increasing count (e.g., total requests).
    Counter(u64),

    /// A text value (e.g., status, last log line).
    Text(String),

    /// A channel receiver for streaming values (tail -f).
    #[allow(dead_code)]
    Stream(broadcast::Receiver<String>),
}

impl MetricValue {
    /// Converts the metric value to plain text.
    pub fn to_plain(&self) -> String {
        match self {
            MetricValue::Gauge(v) => format!("{:.2}", v),
            MetricValue::Counter(v) => v.to_string(),
            MetricValue::Text(s) => s.clone(),
            MetricValue::Stream(_) => "[stream]".to_string(),
        }
    }

    /// Converts the metric value to JSON format.
    pub fn to_json(&self) -> String {
        let timestamp = chrono::Utc::now().to_rfc3339();

        match self {
            MetricValue::Gauge(v) => {
                format!(
                    r#"{{"value":{},"type":"gauge","timestamp":"{}"}}"#,
                    v, timestamp
                )
            }
            MetricValue::Counter(v) => {
                format!(
                    r#"{{"value":{},"type":"counter","timestamp":"{}"}}"#,
                    v, timestamp
                )
            }
            MetricValue::Text(s) => {
                let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
                format!(
                    r#"{{"value":"{}","type":"text","timestamp":"{}"}}"#,
                    escaped, timestamp
                )
            }
            MetricValue::Stream(_) => {
                format!(
                    r#"{{"value":null,"type":"stream","timestamp":"{}"}}"#,
                    timestamp
                )
            }
        }
    }
}

/// Controls how metric values are formatted when read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    #[default]
    Plain,
    Json,
}

impl OutputFormat {
    /// Parses a string into an OutputFormat.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "plain" => Some(OutputFormat::Plain),
            "json" => Some(OutputFormat::Json),
            _ => None,
        }
    }
}

impl std::str::FromStr for OutputFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).ok_or_else(|| format!("invalid output format: {}", s))
    }
}

/// Trait implemented by all metric collectors.
///
/// Implementors must be `Send + Sync` for thread-safe access from FUSE.
pub trait MetricProvider: Send + Sync {
    /// Returns the path where this metric appears (e.g., "system/cpu/usage").
    fn path(&self) -> &str;

    /// Collects and returns the current metric value.
    fn collect(&self) -> anyhow::Result<MetricValue>;

    /// Returns whether this metric supports streaming (tail -f).
    fn supports_stream(&self) -> bool {
        false
    }

    /// Returns a receiver for streaming values.
    fn stream(&self) -> Option<broadcast::Receiver<String>> {
        None
    }
}

/// Represents a node in the virtual filesystem tree.
pub enum FsNode {
    /// A directory containing child nodes.
    Directory { children: HashMap<String, FsNode> },

    /// A metric file that returns a value when read.
    Metric { provider: Arc<dyn MetricProvider> },

    /// A writable configuration file.
    Config {
        value: String,
        on_change: Box<dyn Fn(&str) + Send + Sync>,
    },
}

impl FsNode {
    /// Creates a new empty directory.
    pub fn new_directory() -> Self {
        FsNode::Directory {
            children: HashMap::new(),
        }
    }

    /// Creates a new metric node.
    pub fn new_metric(provider: Arc<dyn MetricProvider>) -> Self {
        FsNode::Metric { provider }
    }

    /// Creates a new config node with a change callback.
    pub fn new_config<F>(initial_value: String, on_change: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        FsNode::Config {
            value: initial_value,
            on_change: Box::new(on_change),
        }
    }

    pub fn is_directory(&self) -> bool {
        matches!(self, FsNode::Directory { .. })
    }

    pub fn is_metric(&self) -> bool {
        matches!(self, FsNode::Metric { .. })
    }

    pub fn is_config(&self) -> bool {
        matches!(self, FsNode::Config { .. })
    }

    pub fn is_writable(&self) -> bool {
        self.is_config()
    }

    pub fn children(&self) -> Option<&HashMap<String, FsNode>> {
        match self {
            FsNode::Directory { children } => Some(children),
            _ => None,
        }
    }

    pub fn children_mut(&mut self) -> Option<&mut HashMap<String, FsNode>> {
        match self {
            FsNode::Directory { children } => Some(children),
            _ => None,
        }
    }
}

impl std::fmt::Debug for FsNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FsNode::Directory { children } => f
                .debug_struct("Directory")
                .field("children", children)
                .finish(),
            FsNode::Metric { provider } => f
                .debug_struct("Metric")
                .field("path", &provider.path())
                .finish(),
            FsNode::Config { value, .. } => f
                .debug_struct("Config")
                .field("value", value)
                .field("on_change", &"<callback>")
                .finish(),
        }
    }
}

// =============================================================================
// DYNAMIC HANDLER
// =============================================================================

/// Trait for handling dynamic paths that aren't statically registered.
///
/// This is used for paths like `/obs/proc/[pid]` where the entries
/// are determined at runtime based on system state.
///
/// ## Example
///
/// ```rust,ignore
/// struct ProcessHandler;
///
/// impl DynamicHandler for ProcessHandler {
///     fn prefix(&self) -> &str {
///         "proc"
///     }
///
///     fn list_entries(&self) -> Vec<String> {
///         // Return list of PIDs
///         vec!["1".into(), "1234".into(), "5678".into()]
///     }
///
///     fn exists(&self, subpath: &str) -> bool {
///         // Check if PID exists
///         subpath.parse::<u32>().ok()
///             .map(|pid| Path::new(&format!("/proc/{}", pid)).exists())
///             .unwrap_or(false)
///     }
///
///     fn read(&self, subpath: &str) -> Option<String> {
///         // Return process info
///         let pid: u32 = subpath.parse().ok()?;
///         Some(format!("Info for PID {}", pid))
///     }
/// }
/// ```
pub trait DynamicHandler: Send + Sync {
    /// The path prefix this handler responds to (e.g., "proc").
    ///
    /// Requests to `/obs/proc/1234` will be routed to the handler
    /// with prefix "proc", and subpath "1234".
    fn prefix(&self) -> &str;

    /// Lists all entries under this prefix.
    ///
    /// Used for `readdir` operations. Return the names only, not full paths.
    fn list_entries(&self) -> Vec<String>;

    /// Checks if a subpath exists under this prefix.
    ///
    /// For example, for path "proc/1234", subpath would be "1234".
    fn exists(&self, subpath: &str) -> bool;

    /// Reads the content for a subpath.
    ///
    /// Returns None if the path doesn't exist or can't be read.
    fn read(&self, subpath: &str) -> Option<String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metric_value_to_plain() {
        assert_eq!(MetricValue::Gauge(23.456).to_plain(), "23.46");
        assert_eq!(MetricValue::Gauge(100.0).to_plain(), "100.00");
        assert_eq!(MetricValue::Counter(1_234_567).to_plain(), "1234567");
        assert_eq!(
            MetricValue::Text("healthy".to_string()).to_plain(),
            "healthy"
        );
    }

    #[test]
    fn test_metric_value_to_json() {
        let json = MetricValue::Gauge(23.4).to_json();
        assert!(json.contains(r#""value":23.4"#));
        assert!(json.contains(r#""type":"gauge""#));
        assert!(json.contains(r#""timestamp":"#));
    }

    #[test]
    fn test_output_format_parsing() {
        assert_eq!(OutputFormat::parse("plain"), Some(OutputFormat::Plain));
        assert_eq!(OutputFormat::parse("json"), Some(OutputFormat::Json));
        assert_eq!(OutputFormat::parse("JSON"), Some(OutputFormat::Json));
        assert_eq!(OutputFormat::parse("invalid"), None);
    }

    #[test]
    fn test_fsnode_construction() {
        let dir = FsNode::new_directory();
        assert!(dir.is_directory());
        assert!(!dir.is_writable());

        let config = FsNode::new_config("value".to_string(), |_| {});
        assert!(config.is_config());
        assert!(config.is_writable());
    }
}
