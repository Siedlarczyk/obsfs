//! FUSE filesystem implementation for ObsFS.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use fuser::{
    FileAttr, FileType, Filesystem, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry, ReplyOpen,
    Request,
};

use obsfs_core::{DynamicHandler, FsNode, OutputFormat, Registry};

use crate::inode::{join_path, InodeTable, ROOT_INODE};

const ENTRY_TTL: Duration = Duration::from_secs(1);
const ATTR_TTL: Duration = Duration::from_secs(1);
const BLOCK_SIZE: u32 = 512;

/// We report a fake size for metrics since actual size is only known at read time.
/// This is necessary because the kernel may skip read() for zero-sized files.
const METRIC_REPORTED_SIZE: u64 = 4096;

/// The main FUSE filesystem implementation.
pub struct ObsFs {
    registry: Arc<RwLock<Registry>>,
    inodes: Arc<RwLock<InodeTable>>,
    format: Arc<RwLock<OutputFormat>>,
    dynamic_handlers: Arc<RwLock<HashMap<String, Arc<dyn DynamicHandler>>>>,
    uid: u32,
    gid: u32,
}

impl ObsFs {
    pub fn new(registry: Registry) -> Self {
        let uid = nix::unistd::getuid().as_raw();
        let gid = nix::unistd::getgid().as_raw();

        Self {
            registry: Arc::new(RwLock::new(registry)),
            inodes: Arc::new(RwLock::new(InodeTable::new())),
            format: Arc::new(RwLock::new(OutputFormat::Plain)),
            dynamic_handlers: Arc::new(RwLock::new(HashMap::new())),
            uid,
            gid,
        }
    }

    /// Registers a dynamic handler for a path prefix.
    ///
    /// For example, registering a handler with prefix "proc" will handle
    /// all paths under /obs/proc/*.
    pub fn register_dynamic_handler(&mut self, handler: Arc<dyn DynamicHandler>) {
        let prefix = handler.prefix().to_string();
        match self.dynamic_handlers.write() {
            Ok(mut handlers) => {
                handlers.insert(prefix, handler);
            }
            Err(e) => {
                tracing::warn!(
                    "Dynamic handlers lock poisoned, recovering with potentially stale data"
                );
                let mut handlers = e.into_inner();
                handlers.insert(prefix, handler);
            }
        }
    }

    fn get_dynamic_handler(&self, path: &str) -> Option<(Arc<dyn DynamicHandler>, String)> {
        let handlers = match self.dynamic_handlers.read() {
            Ok(h) => h,
            Err(_) => {
                tracing::warn!(
                    "Dynamic handlers lock poisoned during read, skipping dynamic handlers"
                );
                return None;
            }
        };

        // Check if path starts with any handler prefix
        for (prefix, handler) in handlers.iter() {
            if path == *prefix {
                // Exact match - this is the directory itself
                return Some((Arc::clone(handler), String::new()));
            }
            if path.starts_with(&format!("{}/", prefix)) {
                let subpath = path.strip_prefix(&format!("{}/", prefix)).unwrap_or("");
                return Some((Arc::clone(handler), subpath.to_string()));
            }
        }

        None
    }

    fn dir_attr(&self, ino: u64) -> FileAttr {
        FileAttr {
            ino,
            size: 0,
            blocks: 0,
            atime: SystemTime::now(),
            mtime: SystemTime::now(),
            ctime: SystemTime::now(),
            crtime: UNIX_EPOCH,
            kind: FileType::Directory,
            perm: 0o755,
            nlink: 2,
            uid: self.uid,
            gid: self.gid,
            rdev: 0,
            blksize: BLOCK_SIZE,
            flags: 0,
        }
    }

    fn metric_attr(&self, ino: u64, size: u64) -> FileAttr {
        FileAttr {
            ino,
            size,
            blocks: size.div_ceil(BLOCK_SIZE as u64),
            atime: SystemTime::now(),
            mtime: SystemTime::now(),
            ctime: SystemTime::now(),
            crtime: UNIX_EPOCH,
            kind: FileType::RegularFile,
            perm: 0o444,
            nlink: 1,
            uid: self.uid,
            gid: self.gid,
            rdev: 0,
            blksize: BLOCK_SIZE,
            flags: 0,
        }
    }

    fn config_attr(&self, ino: u64, size: u64) -> FileAttr {
        FileAttr {
            ino,
            size,
            blocks: size.div_ceil(BLOCK_SIZE as u64),
            atime: SystemTime::now(),
            mtime: SystemTime::now(),
            ctime: SystemTime::now(),
            crtime: UNIX_EPOCH,
            kind: FileType::RegularFile,
            perm: 0o644,
            nlink: 1,
            uid: self.uid,
            gid: self.gid,
            rdev: 0,
            blksize: BLOCK_SIZE,
            flags: 0,
        }
    }

    fn get_content(&self, node: &FsNode, _path: &str) -> anyhow::Result<Vec<u8>> {
        match node {
            FsNode::Directory { .. } => Err(anyhow::anyhow!("cannot read directory")),

            FsNode::Metric { provider } => {
                let format = match self.format.read() {
                    Ok(f) => *f,
                    Err(e) => {
                        // Format lock is poisoned, use stale data
                        tracing::warn!("Format lock poisoned, using recovered data");
                        *e.into_inner()
                    }
                };

                let value = match provider.collect() {
                    Ok(metric_value) => {
                        let formatted = match format {
                            OutputFormat::Plain => metric_value.to_plain(),
                            OutputFormat::Json => metric_value.to_json(),
                        };
                        format!("{}\n", formatted)
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to collect metric");
                        format!("error: {}\n", e)
                    }
                };

                Ok(value.into_bytes())
            }

            FsNode::Config { value, .. } => Ok(format!("{}\n", value).into_bytes()),
        }
    }

    fn ensure_inode(&self, path: &str) -> u64 {
        match self.inodes.write() {
            Ok(mut inodes) => inodes.get_or_allocate(path),
            Err(e) => {
                tracing::error!("Inode table lock poisoned during ensure_inode");
                let mut inodes = e.into_inner();
                inodes.get_or_allocate(path)
            }
        }
    }
}

impl Filesystem for ObsFs {
    fn init(
        &mut self,
        _req: &Request<'_>,
        _config: &mut fuser::KernelConfig,
    ) -> Result<(), libc::c_int> {
        tracing::info!("ObsFS filesystem initialized");
        Ok(())
    }

    fn destroy(&mut self) {
        tracing::info!("ObsFS filesystem destroyed");
    }

    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let name = match name.to_str() {
            Some(n) => n,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        let parent_path = {
            match self.inodes.read() {
                Ok(inodes) => match inodes.resolve_path(parent) {
                    Some(p) => p.to_string(),
                    None => {
                        reply.error(libc::ENOENT);
                        return;
                    }
                },
                Err(_) => {
                    tracing::error!("Inode table lock poisoned during lookup");
                    reply.error(libc::EIO);
                    return;
                }
            }
        };

        let child_path = join_path(&parent_path, name);

        // First, check static registry
        match self.registry.read() {
            Ok(registry) => {
                if let Some(node) = registry.get(&child_path) {
                    let ino = self.ensure_inode(&child_path);
                    let attr = match node {
                        FsNode::Directory { .. } => self.dir_attr(ino),
                        FsNode::Metric { .. } => self.metric_attr(ino, METRIC_REPORTED_SIZE),
                        FsNode::Config { value, .. } => {
                            self.config_attr(ino, value.len() as u64 + 1)
                        }
                    };
                    reply.entry(&ENTRY_TTL, &attr, 0);
                    return;
                }
            }
            Err(_) => {
                tracing::error!("Registry lock poisoned during lookup");
                reply.error(libc::EIO);
                return;
            }
        }

        // Check if this is a dynamic handler prefix (directory)
        {
            match self.dynamic_handlers.read() {
                Ok(handlers) => {
                    if handlers.contains_key(&child_path) {
                        // This is the root directory of a dynamic handler
                        let ino = self.ensure_inode(&child_path);
                        reply.entry(&ENTRY_TTL, &self.dir_attr(ino), 0);
                        return;
                    }
                }
                Err(_) => {
                    tracing::error!("Dynamic handlers lock poisoned during lookup");
                    reply.error(libc::EIO);
                    return;
                }
            }
        }

        // Check dynamic handlers for subpaths
        if let Some((handler, subpath)) = self.get_dynamic_handler(&child_path) {
            if subpath.is_empty() {
                // This is the directory itself
                let ino = self.ensure_inode(&child_path);
                reply.entry(&ENTRY_TTL, &self.dir_attr(ino), 0);
            } else if handler.exists(&subpath) {
                // This is a dynamic entry
                let ino = self.ensure_inode(&child_path);
                reply.entry(&ENTRY_TTL, &self.metric_attr(ino, METRIC_REPORTED_SIZE), 0);
            } else {
                reply.error(libc::ENOENT);
            }
            return;
        }

        reply.error(libc::ENOENT);
    }

    fn getattr(&mut self, _req: &Request, ino: u64, reply: ReplyAttr) {
        let path = {
            match self.inodes.read() {
                Ok(inodes) => match inodes.resolve_path(ino) {
                    Some(p) => p.to_string(),
                    None => {
                        reply.error(libc::ENOENT);
                        return;
                    }
                },
                Err(_) => {
                    tracing::error!("Inode table lock poisoned during getattr");
                    reply.error(libc::EIO);
                    return;
                }
            }
        };

        // Check static registry first
        match self.registry.read() {
            Ok(registry) => {
                if let Some(node) = registry.get(&path) {
                    let attr = match node {
                        FsNode::Directory { .. } => self.dir_attr(ino),
                        FsNode::Metric { .. } => self.metric_attr(ino, METRIC_REPORTED_SIZE),
                        FsNode::Config { value, .. } => {
                            self.config_attr(ino, value.len() as u64 + 1)
                        }
                    };
                    reply.attr(&ATTR_TTL, &attr);
                    return;
                }
            }
            Err(_) => {
                tracing::error!("Registry lock poisoned during getattr");
                reply.error(libc::EIO);
                return;
            }
        }

        // Check if this is a dynamic handler prefix (directory)
        {
            match self.dynamic_handlers.read() {
                Ok(handlers) => {
                    if handlers.contains_key(&path) {
                        reply.attr(&ATTR_TTL, &self.dir_attr(ino));
                        return;
                    }
                }
                Err(_) => {
                    tracing::error!("Dynamic handlers lock poisoned during getattr");
                    reply.error(libc::EIO);
                    return;
                }
            }
        }

        // Check dynamic handlers
        if let Some((handler, subpath)) = self.get_dynamic_handler(&path) {
            if subpath.is_empty() {
                reply.attr(&ATTR_TTL, &self.dir_attr(ino));
            } else if handler.exists(&subpath) {
                reply.attr(&ATTR_TTL, &self.metric_attr(ino, METRIC_REPORTED_SIZE));
            } else {
                reply.error(libc::ENOENT);
            }
            return;
        }

        reply.error(libc::ENOENT);
    }

    fn open(&mut self, _req: &Request, _ino: u64, _flags: i32, reply: ReplyOpen) {
        // FOPEN_DIRECT_IO bypasses kernel page cache, ensuring read() is always called.
        const FOPEN_DIRECT_IO: u32 = 1;
        reply.opened(0, FOPEN_DIRECT_IO);
    }

    fn readdir(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        let path = {
            match self.inodes.read() {
                Ok(inodes) => match inodes.resolve_path(ino) {
                    Some(p) => p.to_string(),
                    None => {
                        reply.error(libc::ENOENT);
                        return;
                    }
                },
                Err(_) => {
                    tracing::error!("Inode table lock poisoned during readdir");
                    reply.error(libc::EIO);
                    return;
                }
            }
        };

        let mut entries: Vec<(u64, FileType, String)> = Vec::new();

        entries.push((ino, FileType::Directory, ".".to_string()));

        let parent_ino = if path.is_empty() {
            ROOT_INODE
        } else {
            let parent_path = path.rsplit_once('/').map(|(p, _)| p).unwrap_or("");
            self.ensure_inode(parent_path)
        };
        entries.push((parent_ino, FileType::Directory, "..".to_string()));

        // Check if this is a dynamic handler directory
        {
            match self.dynamic_handlers.read() {
                Ok(handlers) => {
                    if let Some(handler) = handlers.get(&path) {
                        // List dynamic entries
                        for name in handler.list_entries() {
                            let child_path = join_path(&path, &name);
                            let child_ino = self.ensure_inode(&child_path);
                            entries.push((child_ino, FileType::RegularFile, name));
                        }

                        for (i, (ino, kind, name)) in
                            entries.iter().enumerate().skip(offset as usize)
                        {
                            let full = reply.add(*ino, (i + 1) as i64, *kind, name);
                            if full {
                                break;
                            }
                        }
                        reply.ok();
                        return;
                    }
                }
                Err(_) => {
                    tracing::error!("Dynamic handlers lock poisoned during readdir");
                    reply.error(libc::EIO);
                    return;
                }
            }
        }

        // Check static registry
        match self.registry.read() {
            Ok(registry) => {
                if let Some(node) = registry.get(&path) {
                    let children = match node.children() {
                        Some(c) => c,
                        None => {
                            reply.error(libc::ENOTDIR);
                            return;
                        }
                    };

                    for (name, child_node) in children {
                        let child_path = join_path(&path, name);
                        let child_ino = self.ensure_inode(&child_path);
                        let kind = if child_node.is_directory() {
                            FileType::Directory
                        } else {
                            FileType::RegularFile
                        };
                        entries.push((child_ino, kind, name.clone()));
                    }
                } else {
                    reply.error(libc::ENOENT);
                    return;
                }
            }
            Err(_) => {
                tracing::error!("Registry lock poisoned during readdir");
                reply.error(libc::EIO);
                return;
            }
        }

        // Also add dynamic handler prefixes if we're at root
        // (but skip if already added from static registry to avoid duplicates)
        if path.is_empty() {
            match self.dynamic_handlers.read() {
                Ok(handlers) => {
                    for prefix in handlers.keys() {
                        // Skip if this prefix already exists as a static directory
                        if !entries.iter().any(|(_, _, name)| name == prefix) {
                            let child_ino = self.ensure_inode(prefix);
                            entries.push((child_ino, FileType::Directory, prefix.clone()));
                        }
                    }
                }
                Err(_) => {
                    tracing::error!("Dynamic handlers lock poisoned during readdir (phase 2)");
                    reply.error(libc::EIO);
                    return;
                }
            }
        }

        for (i, (ino, kind, name)) in entries.iter().enumerate().skip(offset as usize) {
            let full = reply.add(*ino, (i + 1) as i64, *kind, name);
            if full {
                break;
            }
        }

        reply.ok();
    }

    fn read(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        size: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyData,
    ) {
        let path = {
            match self.inodes.read() {
                Ok(inodes) => match inodes.resolve_path(ino) {
                    Some(p) => p.to_string(),
                    None => {
                        reply.error(libc::ENOENT);
                        return;
                    }
                },
                Err(_) => {
                    tracing::error!("Inode table lock poisoned during read");
                    reply.error(libc::EIO);
                    return;
                }
            }
        };

        // Check dynamic handlers first
        if let Some((handler, subpath)) = self.get_dynamic_handler(&path) {
            if !subpath.is_empty() {
                if let Some(content) = handler.read(&subpath) {
                    let content = format!("{}\n", content).into_bytes();
                    let offset = offset as usize;
                    let size = size as usize;

                    if offset >= content.len() {
                        reply.data(&[]);
                    } else {
                        let end = std::cmp::min(offset + size, content.len());
                        reply.data(&content[offset..end]);
                    }
                    return;
                }
                // If handler returns None, fall through to check static registry
            }
        }

        // Check static registry
        let content = match self.registry.read() {
            Ok(registry) => match registry.get(&path) {
                Some(node) => match self.get_content(node, &path) {
                    Ok(c) => c,
                    Err(_) => {
                        reply.error(libc::EIO);
                        return;
                    }
                },
                None => {
                    reply.error(libc::ENOENT);
                    return;
                }
            },
            Err(_) => {
                tracing::error!("Registry lock poisoned during read");
                reply.error(libc::EIO);
                return;
            }
        };

        let offset = offset as usize;
        let size = size as usize;

        if offset >= content.len() {
            reply.data(&[]);
        } else {
            let end = std::cmp::min(offset + size, content.len());
            reply.data(&content[offset..end]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_obsfs_creation() {
        let registry = Registry::new();
        let _fs = ObsFs::new(registry);
    }
}
