//! Plugin system for ObsFS collectors.
//!
//! This module provides the [`Plugin`] trait that all collectors should implement
//! to integrate with ObsFS in a standardized way.
//!
//! ## Example
//!
//! ```rust,ignore
//! use obsfs_core::{Plugin, Registry, DynamicHandler};
//! use std::sync::Arc;
//!
//! pub struct MyPlugin;
//!
//! impl Plugin for MyPlugin {
//!     fn name(&self) -> &str {
//!         "my-plugin"
//!     }
//!
//!     fn register(&self, registry: &mut Registry) -> anyhow::Result<()> {
//!         // Register metric providers here
//!         Ok(())
//!     }
//! }
//! ```

use std::sync::Arc;

use crate::{DynamicHandler, Registry};

/// Trait implemented by all ObsFS plugins.
///
/// Plugins are the primary way to extend ObsFS with new metrics and handlers.
/// Each plugin can register static metric providers and/or dynamic handlers.
pub trait Plugin: Send + Sync {
    /// Returns the unique name of this plugin.
    ///
    /// Used for logging and identification.
    fn name(&self) -> &str;

    /// Returns a human-readable description of what this plugin provides.
    fn description(&self) -> &str {
        ""
    }

    /// Registers static metric providers with the registry.
    ///
    /// This is called once during startup to register all metrics
    /// that this plugin provides.
    fn register(&self, registry: &mut Registry) -> anyhow::Result<()>;

    /// Returns dynamic handlers provided by this plugin.
    ///
    /// Dynamic handlers respond to paths that aren't statically registered,
    /// like `/obs/proc/[pid]` where entries are determined at runtime.
    ///
    /// Returns an empty vector by default.
    fn dynamic_handlers(&self) -> Vec<Arc<dyn DynamicHandler>> {
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestPlugin;

    impl Plugin for TestPlugin {
        fn name(&self) -> &str {
            "test-plugin"
        }

        fn description(&self) -> &str {
            "A test plugin"
        }

        fn register(&self, _registry: &mut Registry) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn test_plugin_trait() {
        let plugin = TestPlugin;
        assert_eq!(plugin.name(), "test-plugin");
        assert_eq!(plugin.description(), "A test plugin");
        assert!(plugin.dynamic_handlers().is_empty());
    }
}
