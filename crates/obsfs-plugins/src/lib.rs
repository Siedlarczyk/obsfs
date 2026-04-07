//! Metric plugins for ObsFS.
//!
//! This crate provides implementations of [`MetricProvider`] that collect
//! metrics from various sources like `/proc`, `/sys`, and external services.
//!
//! All plugins implement the [`Plugin`] trait for unified registration.

pub use obsfs_core::{DynamicHandler, MetricProvider, MetricValue, Plugin};

pub mod connections;
pub mod docker;
pub mod health;
pub mod proc_info;
pub mod procsys;
pub mod sensors;
pub mod services;
pub mod users;

pub use connections::ConnectionsPlugin;
pub use docker::DockerPlugin;
pub use health::HealthPlugin;
pub use proc_info::{ProcessInfoPlugin, ProcessInfoProvider};
pub use procsys::ProcSysPlugin;
pub use sensors::SensorsPlugin;
pub use services::ServicesPlugin;
pub use users::UsersPlugin;
