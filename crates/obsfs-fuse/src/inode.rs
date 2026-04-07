//! Bidirectional mapping between filesystem paths and inode numbers.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

/// The inode number for the root directory.
pub const ROOT_INODE: u64 = 1;

const FIRST_ALLOCATABLE_INODE: u64 = 2;

/// Bidirectional mapping between filesystem paths and inode numbers.
///
/// FUSE operations use inodes, but our registry uses paths. This table
/// provides O(1) translation in both directions.
#[derive(Debug)]
pub struct InodeTable {
    path_to_inode: HashMap<String, u64>,
    inode_to_path: HashMap<u64, String>,
    next_inode: AtomicU64,
}

impl InodeTable {
    /// Creates a new inode table with the root directory pre-registered.
    pub fn new() -> Self {
        let mut path_to_inode = HashMap::new();
        let mut inode_to_path = HashMap::new();

        path_to_inode.insert(String::new(), ROOT_INODE);
        inode_to_path.insert(ROOT_INODE, String::new());

        Self {
            path_to_inode,
            inode_to_path,
            next_inode: AtomicU64::new(FIRST_ALLOCATABLE_INODE),
        }
    }

    /// Allocates a new inode for the given path, or returns existing one.
    pub fn allocate(&mut self, path: &str) -> u64 {
        if let Some(&inode) = self.path_to_inode.get(path) {
            return inode;
        }

        let inode = self.next_inode.fetch_add(1, Ordering::SeqCst);

        self.path_to_inode.insert(path.to_string(), inode);
        self.inode_to_path.insert(inode, path.to_string());

        inode
    }

    /// Returns the path for a given inode number.
    pub fn resolve_path(&self, inode: u64) -> Option<&str> {
        self.inode_to_path.get(&inode).map(|s| s.as_str())
    }

    /// Returns the inode for a given path.
    pub fn resolve_inode(&self, path: &str) -> Option<u64> {
        self.path_to_inode.get(path).copied()
    }

    /// Checks if an inode is registered.
    pub fn contains_inode(&self, inode: u64) -> bool {
        self.inode_to_path.contains_key(&inode)
    }

    /// Checks if a path is registered.
    pub fn contains_path(&self, path: &str) -> bool {
        self.path_to_inode.contains_key(path)
    }

    /// Returns the number of registered paths (including root).
    pub fn len(&self) -> usize {
        self.path_to_inode.len()
    }

    /// Returns `true` if only the root is registered.
    pub fn is_empty(&self) -> bool {
        self.len() == 1
    }

    /// Removes a path from the table. The inode number is not reused.
    pub fn remove(&mut self, path: &str) -> Option<u64> {
        if let Some(inode) = self.path_to_inode.remove(path) {
            self.inode_to_path.remove(&inode);
            Some(inode)
        } else {
            None
        }
    }

    /// Gets or allocates an inode for a path.
    pub fn get_or_allocate(&mut self, path: &str) -> u64 {
        self.resolve_inode(path)
            .unwrap_or_else(|| self.allocate(path))
    }
}

impl Default for InodeTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Joins a parent path with a child name.
pub fn join_path(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{}/{}", parent, name)
    }
}

/// Splits a path into parent and name.
pub fn split_path(path: &str) -> (&str, &str) {
    match path.rfind('/') {
        Some(pos) => (&path[..pos], &path[pos + 1..]),
        None => ("", path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_table_has_root() {
        let table = InodeTable::new();
        assert_eq!(table.resolve_inode(""), Some(ROOT_INODE));
        assert_eq!(table.resolve_path(ROOT_INODE), Some(""));
    }

    #[test]
    fn test_allocation() {
        let mut table = InodeTable::new();

        let inode1 = table.allocate("a");
        let inode2 = table.allocate("b");
        let inode3 = table.allocate("a");

        assert_eq!(inode1, 2);
        assert_eq!(inode2, 3);
        assert_eq!(inode3, inode1);
    }

    #[test]
    fn test_resolve() {
        let mut table = InodeTable::new();
        table.allocate("test/path");

        assert_eq!(table.resolve_inode("test/path"), Some(2));
        assert_eq!(table.resolve_path(2), Some("test/path"));
        assert_eq!(table.resolve_inode("nonexistent"), None);
    }

    #[test]
    fn test_remove() {
        let mut table = InodeTable::new();

        let inode = table.allocate("test");
        assert!(table.contains_path("test"));

        let removed = table.remove("test");
        assert_eq!(removed, Some(inode));
        assert!(!table.contains_path("test"));
    }

    #[test]
    fn test_join_path() {
        assert_eq!(join_path("", "child"), "child");
        assert_eq!(join_path("parent", "child"), "parent/child");
    }

    #[test]
    fn test_split_path() {
        assert_eq!(split_path("child"), ("", "child"));
        assert_eq!(split_path("parent/child"), ("parent", "child"));
    }
}
