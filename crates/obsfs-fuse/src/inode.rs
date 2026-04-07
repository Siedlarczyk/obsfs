//! Bidirectional mapping between filesystem paths and inode numbers.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

/// The inode number for the root directory.
pub const ROOT_INODE: u64 = 1;

const FIRST_ALLOCATABLE_INODE: u64 = 2;

/// Bidirectional mapping between filesystem paths and inode numbers.
///
/// FUSE operations use inodes, but our registry uses paths. This table
/// provides O(1) translation in both directions.
///
/// Supports soft-delete with periodic sweep to handle dynamic paths without
/// memory leaks. Stale inodes are marked but not immediately removed, then
/// swept periodically based on age.
#[derive(Debug)]
pub struct InodeTable {
    path_to_inode: HashMap<String, u64>,
    inode_to_path: HashMap<u64, String>,
    next_inode: AtomicU64,
    /// Tracks inodes marked as stale with their timestamp
    stale: HashMap<u64, SystemTime>,
}

impl InodeTable {
    pub fn new() -> Self {
        let mut path_to_inode = HashMap::new();
        let mut inode_to_path = HashMap::new();

        path_to_inode.insert(String::new(), ROOT_INODE);
        inode_to_path.insert(ROOT_INODE, String::new());

        Self {
            path_to_inode,
            inode_to_path,
            next_inode: AtomicU64::new(FIRST_ALLOCATABLE_INODE),
            stale: HashMap::new(),
        }
    }

    pub fn allocate(&mut self, path: &str) -> u64 {
        if let Some(&inode) = self.path_to_inode.get(path) {
            return inode;
        }

        let inode = self.next_inode.fetch_add(1, Ordering::SeqCst);

        self.path_to_inode.insert(path.to_string(), inode);
        self.inode_to_path.insert(inode, path.to_string());

        inode
    }

    /// Returns None if the inode doesn't exist or is stale.
    pub fn resolve_path(&self, inode: u64) -> Option<&str> {
        // Stale inodes should appear as if they don't exist
        if self.is_stale(inode) {
            return None;
        }
        self.inode_to_path.get(&inode).map(|s| s.as_str())
    }

    pub fn resolve_inode(&self, path: &str) -> Option<u64> {
        self.path_to_inode.get(path).copied()
    }

    pub fn contains_inode(&self, inode: u64) -> bool {
        self.inode_to_path.contains_key(&inode)
    }

    pub fn contains_path(&self, path: &str) -> bool {
        self.path_to_inode.contains_key(path)
    }

    pub fn len(&self) -> usize {
        self.path_to_inode.len()
    }

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

    /// Marks an inode as stale. Operations on stale inodes will return ENOENT.
    ///
    /// This is used for soft-delete - the inode is not immediately removed,
    /// but marked for later cleanup via `sweep_stale()`.
    pub fn mark_stale(&mut self, inode: u64) {
        if inode == ROOT_INODE {
            // Never mark root as stale
            return;
        }
        self.stale.insert(inode, SystemTime::now());
    }

    pub fn is_stale(&self, inode: u64) -> bool {
        self.stale.contains_key(&inode)
    }

    /// Sweeps stale inodes older than `max_age` and removes them completely.
    ///
    /// This is typically called periodically (e.g., every few seconds) to
    /// clean up inodes that have been marked as stale for a sufficient time.
    ///
    /// Returns the number of inodes removed.
    pub fn sweep_stale(&mut self, max_age: Duration) -> usize {
        let now = SystemTime::now();
        let mut to_remove = Vec::new();

        // Find inodes that are old enough to remove
        for (&inode, &marked_time) in self.stale.iter() {
            if let Ok(age) = now.duration_since(marked_time) {
                if age >= max_age {
                    to_remove.push(inode);
                }
            }
        }

        // Remove the old stale entries
        let removed_count = to_remove.len();
        for inode in to_remove {
            self.stale.remove(&inode);
            if let Some(path) = self.inode_to_path.remove(&inode) {
                self.path_to_inode.remove(&path);
            }
        }

        removed_count
    }

    pub fn stale_count(&self) -> usize {
        self.stale.len()
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

    #[test]
    fn test_mark_stale() {
        let mut table = InodeTable::new();
        let inode = table.allocate("test");

        assert!(!table.is_stale(inode));
        table.mark_stale(inode);
        assert!(table.is_stale(inode));
    }

    #[test]
    fn test_root_cannot_be_stale() {
        let mut table = InodeTable::new();
        table.mark_stale(ROOT_INODE);
        assert!(!table.is_stale(ROOT_INODE));
    }

    #[test]
    fn test_sweep_stale() {
        let mut table = InodeTable::new();
        let inode1 = table.allocate("path1");
        let inode2 = table.allocate("path2");

        table.mark_stale(inode1);
        table.mark_stale(inode2);
        assert_eq!(table.stale_count(), 2);

        // Sweep with very short max_age - should remove both
        let removed = table.sweep_stale(Duration::from_secs(0));
        assert_eq!(removed, 2);
        assert_eq!(table.stale_count(), 0);

        // Both inodes should be removed from the table
        assert!(!table.contains_inode(inode1));
        assert!(!table.contains_inode(inode2));
    }

    #[test]
    fn test_sweep_respects_max_age() {
        use std::thread;

        let mut table = InodeTable::new();
        let inode = table.allocate("old_path");
        table.mark_stale(inode);

        // Sweep with a long max_age - should not remove yet
        let removed = table.sweep_stale(Duration::from_secs(10));
        assert_eq!(removed, 0);
        assert!(table.is_stale(inode));

        // Sleep a tiny bit and try again with a very short max_age
        thread::sleep(Duration::from_millis(10));
        let removed = table.sweep_stale(Duration::from_millis(5));
        assert_eq!(removed, 1);
        assert!(!table.is_stale(inode));
    }
}
