//! FUSE filesystem implementation for ObsFS.
//!
//! This crate bridges the Linux kernel's VFS layer and the metric collection system.

pub mod fs;
pub mod inode;

pub use fs::ObsFs;
pub use inode::InodeTable;
