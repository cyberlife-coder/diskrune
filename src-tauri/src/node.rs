use serde::{Deserialize, Serialize};

/// A node in the scanned directory tree.
/// Represents either a directory (with children) or a file (leaf).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct DirNode {
    pub name:       String,
    pub path:       String,
    pub size:       u64,
    pub is_dir:     bool,
    pub dir_count:  usize,
    pub file_count: usize,
    pub children:   Vec<DirNode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl DirNode {
    pub fn new_dir(name: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            name:       name.into(),
            path:       path.into(),
            size:       0,
            is_dir:     true,
            dir_count:  0,
            file_count: 0,
            children:   Vec::new(),
            error:      None,
        }
    }

    pub fn new_file(name: impl Into<String>, path: impl Into<String>, size: u64) -> Self {
        Self {
            name:       name.into(),
            path:       path.into(),
            size,
            is_dir:     false,
            dir_count:  0,
            file_count: 0,
            children:   Vec::new(),
            error:      None,
        }
    }

    pub fn with_error(mut self, msg: impl Into<String>) -> Self {
        self.error = Some(msg.into());
        self
    }
}

// ── Aggregate stats in a single pass ─────────────────────────────────────────
// Returns (total_size, total_subdirs, total_files).
// Replaces three separate O(n) iterations with one fold.
pub struct Stats {
    pub size:       u64,
    pub dir_count:  usize,
    pub file_count: usize,
}

pub fn aggregate(children: &[DirNode]) -> Stats {
    let (size, dir_count, file_count) =
        children.iter().fold((0u64, 0usize, 0usize), |(sz, dirs, files), c| {
            if c.is_dir {
                (sz + c.size, dirs + 1 + c.dir_count, files + c.file_count)
            } else {
                (sz + c.size, dirs, files + 1)
            }
        });
    Stats { size, dir_count, file_count }
}

/// Sorts nodes largest-first (unstable sort is faster; stable order not needed).
pub fn sort_by_size_desc(nodes: &mut Vec<DirNode>) {
    nodes.sort_unstable_by(|a, b| b.size.cmp(&a.size));
}

// ── Keep individual helpers for backwards-compat and unit tests ───────────────

pub fn total_size(children: &[DirNode]) -> u64 {
    children.iter().map(|c| c.size).sum()
}

pub fn count_subdirs(children: &[DirNode]) -> usize {
    children.iter().map(|c| if c.is_dir { 1 + c.dir_count } else { 0 }).sum()
}

pub fn count_files(children: &[DirNode]) -> usize {
    children.iter().map(|c| if c.is_dir { c.file_count } else { 1 }).sum()
}

// ── Tests ─────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    fn file(size: u64) -> DirNode { DirNode::new_file("f.txt", "/p/f.txt", size) }

    fn dir_with_size(size: u64) -> DirNode {
        let mut d = DirNode::new_dir("sub", "/p/sub");
        d.size = size;
        d
    }

    #[test]
    fn aggregate_sums_sizes_and_counts() {
        let children = vec![
            file(100),
            {
                let mut d = DirNode::new_dir("sub", "/p/sub");
                d.size       = 500;
                d.dir_count  = 2;
                d.file_count = 3;
                d
            },
            file(200),
        ];
        let s = aggregate(&children);
        assert_eq!(s.size,       800);   // 100 + 500 + 200
        assert_eq!(s.dir_count,  3);     // 1 direct + 2 nested
        assert_eq!(s.file_count, 5);     // 2 direct + 3 nested
    }

    #[test]
    fn aggregate_empty_slice_returns_zeros() {
        let s = aggregate(&[]);
        assert_eq!(s.size, 0);
        assert_eq!(s.dir_count, 0);
        assert_eq!(s.file_count, 0);
    }

    #[test]
    fn total_size_sums_all_children() {
        assert_eq!(total_size(&[file(100), file(200), file(300)]), 600);
    }

    #[test]
    fn total_size_is_zero_for_empty_slice() {
        assert_eq!(total_size(&[]), 0);
    }

    #[test]
    fn count_files_ignores_directories() {
        assert_eq!(count_files(&[file(100), dir_with_size(200), file(50)]), 2);
    }

    #[test]
    fn count_subdirs_counts_direct_dirs() {
        assert_eq!(count_subdirs(&[dir_with_size(100), file(200), dir_with_size(50)]), 2);
    }

    #[test]
    fn count_subdirs_includes_nested_dir_count() {
        let mut nested = DirNode::new_dir("inner", "/p/outer/inner");
        nested.dir_count = 3;
        assert_eq!(count_subdirs(&[nested]), 4); // 1 + 3
    }

    #[test]
    fn sort_by_size_desc_orders_largest_first() {
        let mut nodes = vec![file(100), file(500), file(200)];
        sort_by_size_desc(&mut nodes);
        assert_eq!(nodes[0].size, 500);
        assert_eq!(nodes[1].size, 200);
        assert_eq!(nodes[2].size, 100);
    }

    #[test]
    fn new_dir_defaults_are_correct() {
        let n = DirNode::new_dir("test", "/test");
        assert!(n.is_dir);
        assert_eq!(n.size, 0);
        assert!(n.children.is_empty());
        assert!(n.error.is_none());
    }

    #[test]
    fn with_error_sets_error_field() {
        let n = DirNode::new_dir("test", "/test").with_error("Access denied");
        assert_eq!(n.error.as_deref(), Some("Access denied"));
    }
}
