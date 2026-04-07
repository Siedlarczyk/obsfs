//! Configuration file parsing and validation.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::types::OutputFormat;

/// Main configuration struct for ObsFS.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub mount: MountConfig,
    pub format: FormatConfig,
    pub logging: LoggingConfig,
}

impl Config {
    /// Loads configuration from the default location (`/etc/obsfs/config.toml`).
    pub fn load_default() -> anyhow::Result<Self> {
        let default_path = PathBuf::from("/etc/obsfs/config.toml");

        if default_path.exists() {
            Self::load_from(&default_path)
        } else {
            tracing::info!("No config file found, using defaults");
            Ok(Self::default())
        }
    }

    /// Loads configuration from a specific file path.
    pub fn load_from<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let path = path.as_ref();

        let contents = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("failed to read config file {:?}: {}", path, e))?;

        let config: Config = toml::from_str(&contents)
            .map_err(|e| anyhow::anyhow!("failed to parse config file {:?}: {}", path, e))?;

        tracing::info!(?path, "Loaded configuration");

        Ok(config)
    }

    /// Validates the configuration and returns any errors.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        if self.mount.path.as_os_str().is_empty() {
            errors.push("mount.path cannot be empty".to_string());
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// Configuration for the FUSE mount.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MountConfig {
    pub path: PathBuf,
    pub allow_other: bool,
}

impl Default for MountConfig {
    fn default() -> Self {
        Self {
            path: PathBuf::from("/obs"),
            allow_other: true,
        }
    }
}

/// Configuration for output formatting.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FormatConfig {
    pub default: OutputFormat,
}

impl Default for FormatConfig {
    fn default() -> Self {
        Self {
            default: OutputFormat::Plain,
        }
    }
}

/// Log level for daemon logging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    #[default]
    Warn,
    Error,
}

impl LogLevel {
    /// Returns the tracing filter directive string.
    pub fn as_filter_str(&self) -> &'static str {
        match self {
            LogLevel::Trace => "trace",
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
        }
    }
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_filter_str())
    }
}

/// Log output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    /// Structured JSON output (good for log aggregators).
    #[default]
    Json,
    /// Human-readable pretty output (good for terminal).
    Pretty,
    /// Compact single-line output.
    Compact,
}

/// Log output destination.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogOutput {
    /// Log to stdout.
    Stdout,
    /// Log to stderr.
    #[default]
    Stderr,
    /// Log to a file at the specified path.
    File(std::path::PathBuf),
}

/// Configuration for daemon logging.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    /// Log level filter.
    pub level: LogLevel,
    /// Log output format.
    pub format: LogFormat,
    /// Log output destination.
    pub output: LogOutput,
    /// Whether to include target (module path) in logs.
    pub show_target: bool,
    /// Whether to include file and line number in logs.
    pub show_location: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: LogLevel::Warn,
            format: LogFormat::Compact,
            output: LogOutput::Stderr,
            show_target: false,
            show_location: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_is_valid() {
        let config = Config::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_parse_minimal_config() {
        let toml = r#"
            [mount]
            path = "/custom/obs"
        "#;

        let config: Config = toml::from_str(toml).expect("should parse");
        assert_eq!(config.mount.path, PathBuf::from("/custom/obs"));
        assert_eq!(config.format.default, OutputFormat::Plain);
    }

    #[test]
    fn test_parse_full_config() {
        let toml = r#"
            [mount]
            path = "/obs"
            allow_other = false

            [format]
            default = "json"

            [logging]
            level = "debug"
            format = "pretty"
            output = "stdout"
            show_target = false
            show_location = true
        "#;

        let config: Config = toml::from_str(toml).expect("should parse");

        assert_eq!(config.mount.path, PathBuf::from("/obs"));
        assert!(!config.mount.allow_other);
        assert_eq!(config.format.default, OutputFormat::Json);
        assert_eq!(config.logging.level, LogLevel::Debug);
        assert_eq!(config.logging.format, LogFormat::Pretty);
        assert_eq!(config.logging.output, LogOutput::Stdout);
        assert!(!config.logging.show_target);
        assert!(config.logging.show_location);
    }

    #[test]
    fn test_validation_catches_errors() {
        let mut config = Config::default();
        config.mount.path = PathBuf::new();

        let result = config.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_log_output_file() {
        let toml = r#"
            [logging]
            output = { file = "/var/log/obsfs.log" }
        "#;

        let config: Config = toml::from_str(toml).expect("should parse");
        assert_eq!(
            config.logging.output,
            LogOutput::File(PathBuf::from("/var/log/obsfs.log"))
        );
    }

    #[test]
    fn test_log_levels() {
        for (level_str, expected) in [
            ("trace", LogLevel::Trace),
            ("debug", LogLevel::Debug),
            ("info", LogLevel::Info),
            ("warn", LogLevel::Warn),
            ("error", LogLevel::Error),
        ] {
            let toml = format!(
                r#"
                [logging]
                level = "{}"
            "#,
                level_str
            );

            let config: Config = toml::from_str(&toml).expect("should parse");
            assert_eq!(config.logging.level, expected);
        }
    }
}
