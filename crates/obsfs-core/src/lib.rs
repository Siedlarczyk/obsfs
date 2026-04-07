//! Core types and utilities for the ObsFS observability filesystem.
//!
//! This crate provides the foundational abstractions:
//! - [`types`]: Core type definitions (`MetricValue`, `MetricProvider`, `FsNode`)
//! - [`config`]: Configuration file parsing and validation
//! - [`registry`]: Filesystem node tree management
//! - [`plugin`]: Plugin trait for extending ObsFS
//! - [`utils`]: Utility functions

pub mod config;
pub mod plugin;
pub mod registry;
pub mod types;
pub mod utils;

pub use config::{Config, LogFormat, LogLevel, LogOutput, LoggingConfig};
pub use plugin::Plugin;
pub use registry::Registry;
pub use types::{DynamicHandler, FsNode, MetricProvider, MetricValue, OutputFormat};
pub use utils::format_bytes;
