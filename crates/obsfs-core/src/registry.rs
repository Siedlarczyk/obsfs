//! Filesystem node tree management and path resolution.

use std::sync::Arc;

use crate::types::{FsNode, MetricProvider};

/// Manages the virtual filesystem tree.
pub struct Registry {
    root: FsNode,
}

impl Registry {
    /// Creates a new empty registry.
    pub fn new() -> Self {
        Self {
            root: FsNode::new_directory(),
        }
    }

    /// Returns a reference to the root node.
    pub fn root(&self) -> &FsNode {
        &self.root
    }

    /// Returns a mutable reference to the root node.
    pub fn root_mut(&mut self) -> &mut FsNode {
        &mut self.root
    }

    /// Inserts a node at the given path, creating parent directories as needed.
    pub fn insert(&mut self, path: &str, node: FsNode) -> Result<(), String> {
        if path.is_empty() {
            return Err("cannot replace root node".to_string());
        }

        let components: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

        if components.is_empty() {
            return Err("empty path".to_string());
        }

        let mut current = &mut self.root;

        for (i, component) in components.iter().enumerate().take(components.len() - 1) {
            let children = match current {
                FsNode::Directory { children } => children,
                _ => {
                    let partial_path: String = components[..=i].join("/");
                    return Err(format!(
                        "path conflict: '{}' exists and is not a directory",
                        partial_path
                    ));
                }
            };

            if !children.contains_key(*component) {
                children.insert(component.to_string(), FsNode::new_directory());
            }

            current = children.get_mut(*component).unwrap();
        }

        let final_name = components.last().unwrap();

        match current {
            FsNode::Directory { children } => {
                children.insert(final_name.to_string(), node);
                Ok(())
            }
            _ => Err(format!("parent of '{}' is not a directory", path)),
        }
    }

    /// Inserts a metric provider at its declared path.
    pub fn insert_provider(&mut self, provider: Arc<dyn MetricProvider>) -> Result<(), String> {
        let path = provider.path().to_string();
        let node = FsNode::new_metric(provider);
        self.insert(&path, node)
    }

    /// Looks up a node by path.
    pub fn get(&self, path: &str) -> Option<&FsNode> {
        if path.is_empty() {
            return Some(&self.root);
        }

        let components: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

        let mut current = &self.root;

        for component in components {
            match current {
                FsNode::Directory { children } => {
                    current = children.get(component)?;
                }
                _ => return None,
            }
        }

        Some(current)
    }

    /// Looks up a node by path and returns a mutable reference.
    pub fn get_mut(&mut self, path: &str) -> Option<&mut FsNode> {
        if path.is_empty() {
            return Some(&mut self.root);
        }

        let components: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

        let mut current = &mut self.root;

        for component in components {
            match current {
                FsNode::Directory { children } => {
                    current = children.get_mut(component)?;
                }
                _ => return None,
            }
        }

        Some(current)
    }

    /// Lists the children of a directory.
    pub fn list_children(&self, path: &str) -> Option<Vec<String>> {
        let node = self.get(path)?;

        match node {
            FsNode::Directory { children } => Some(children.keys().cloned().collect()),
            _ => None,
        }
    }

    /// Checks if a path exists in the registry.
    pub fn exists(&self, path: &str) -> bool {
        self.get(path).is_some()
    }

    /// Removes a node from the registry.
    pub fn remove(&mut self, path: &str) -> Option<FsNode> {
        if path.is_empty() {
            return None;
        }

        let components: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

        if components.is_empty() {
            return None;
        }

        let parent_path: String = components[..components.len() - 1].join("/");
        let name = components.last().unwrap();

        let parent = self.get_mut(&parent_path)?;

        match parent {
            FsNode::Directory { children } => children.remove(*name),
            _ => None,
        }
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Registry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Registry")
            .field("root", &self.root)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_get() {
        let mut registry = Registry::new();

        let node = FsNode::new_config("test".to_string(), |_| {});
        registry.insert("a/b/c", node).unwrap();

        assert!(registry.get("a").unwrap().is_directory());
        assert!(registry.get("a/b").unwrap().is_directory());
        assert!(registry.get("a/b/c").unwrap().is_config());
    }

    #[test]
    fn test_auto_create_parents() {
        let mut registry = Registry::new();

        let node = FsNode::new_config("value".to_string(), |_| {});
        registry.insert("very/deep/nested/path", node).unwrap();

        assert!(registry.exists("very"));
        assert!(registry.exists("very/deep"));
        assert!(registry.exists("very/deep/nested"));
        assert!(registry.exists("very/deep/nested/path"));
    }

    #[test]
    fn test_list_children() {
        let mut registry = Registry::new();

        registry
            .insert("dir/a", FsNode::new_config("1".into(), |_| {}))
            .unwrap();
        registry
            .insert("dir/b", FsNode::new_config("2".into(), |_| {}))
            .unwrap();
        registry
            .insert("dir/c", FsNode::new_config("3".into(), |_| {}))
            .unwrap();

        let mut children = registry.list_children("dir").unwrap();
        children.sort();

        assert_eq!(children, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_remove() {
        let mut registry = Registry::new();

        registry
            .insert("a/b", FsNode::new_config("x".into(), |_| {}))
            .unwrap();

        assert!(registry.exists("a/b"));

        let removed = registry.remove("a/b");
        assert!(removed.is_some());
        assert!(!registry.exists("a/b"));
        assert!(registry.exists("a"));
    }

    #[test]
    fn test_cannot_replace_root() {
        let mut registry = Registry::new();
        let result = registry.insert("", FsNode::new_directory());
        assert!(result.is_err());
    }

    #[test]
    fn test_path_conflict() {
        let mut registry = Registry::new();

        registry
            .insert("a", FsNode::new_config("x".into(), |_| {}))
            .unwrap();

        let result = registry.insert("a/b", FsNode::new_config("y".into(), |_| {}));
        assert!(result.is_err());
    }
}
