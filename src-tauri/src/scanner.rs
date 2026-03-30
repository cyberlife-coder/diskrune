use rayon::prelude::*;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::node::{aggregate, sort_by_size_desc, DirNode};

const ACCESS_DENIED: &str = "Access denied";

// ── Progress counter — updated atomically by every scan() call ────────────────
static DIRS_SCANNED: AtomicU64 = AtomicU64::new(0);

/// Reset the counter before a new scan.
pub fn reset_scan_counter() {
    DIRS_SCANNED.store(0, Ordering::Relaxed);
}

/// Current number of directories processed (safe to read from any thread).
pub fn dirs_scanned() -> u64 {
    DIRS_SCANNED.load(Ordering::Relaxed)
}

/// Scans a directory tree in parallel and returns the root DirNode.
///
/// ## Performance design
/// - **Parallel**: Rayon work-stealing distributes subtrees across all CPU cores.
/// - **Syscall-efficient**: uses `file_type()` (free from `readdir` cache) instead of
///   `metadata()` for directory entries — saves one `stat` syscall per subdirectory.
/// - **Symlink-safe**: symlinks are intentionally skipped (they don't represent real
///   disk usage and can cause infinite loops on circular mounts).
/// - **Single-pass stats**: `aggregate()` computes size + dir_count + file_count in
///   one fold instead of three separate iterations.
pub fn scan(root: &Path) -> DirNode {
    DIRS_SCANNED.fetch_add(1, Ordering::Relaxed);

    let name = dir_name(root);
    let path = to_path_string(root);

    match read_entries(root) {
        None         => DirNode::new_dir(name, path).with_error(ACCESS_DENIED),
        Some(entries) => assemble_dir_node(name, path, entries),
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

fn assemble_dir_node(name: String, path: String, entries: Vec<std::fs::DirEntry>) -> DirNode {
    let mut children = build_children_parallel(&entries);
    sort_by_size_desc(&mut children);

    let stats = aggregate(&children);

    let mut node     = DirNode::new_dir(name, path);
    node.size        = stats.size;
    node.dir_count   = stats.dir_count;
    node.file_count  = stats.file_count;
    node.children    = children;
    node
}

fn build_children_parallel(entries: &[std::fs::DirEntry]) -> Vec<DirNode> {
    entries
        .par_iter()
        .filter_map(|entry| {
            // `file_type()` is O(0) on Linux/macOS/Windows — it reuses the d_type
            // already returned by readdir, avoiding an extra stat() syscall.
            let file_type = entry.file_type().ok()?;

            // Skip symlinks: they don't represent real disk usage and following
            // them can produce infinite loops on circular mounts.
            if file_type.is_symlink() {
                return None;
            }

            let path = entry.path();

            if file_type.is_dir() {
                // Recurse — Rayon work-stealing will distribute subtrees
                // across all available CPU cores automatically.
                Some(scan(&path))
            } else {
                // For regular files we still need metadata() to get the size,
                // but we avoid it entirely for directories (saved above).
                let size = entry.metadata().ok().map(|m| m.len()).unwrap_or(0);
                Some(DirNode::new_file(
                    file_name_str(entry),
                    to_path_string(&path),
                    size,
                ))
            }
        })
        .collect()
}

fn read_entries(path: &Path) -> Option<Vec<std::fs::DirEntry>> {
    std::fs::read_dir(path)
        .ok()
        .map(|rd| rd.filter_map(|e| e.ok()).collect())
}

fn dir_name(path: &Path) -> String {
    path.file_name()
        .map(|n| os_str_to_string(n))
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/// Convert OsStr to String: use the zero-copy UTF-8 fast path first,
/// fall back to lossy conversion only for non-UTF-8 paths.
#[inline]
fn os_str_to_string(s: &std::ffi::OsStr) -> String {
    s.to_str().map(|v| v.to_owned()).unwrap_or_else(|| s.to_string_lossy().into_owned())
}

#[inline]
fn file_name_str(entry: &std::fs::DirEntry) -> String {
    os_str_to_string(&entry.file_name())
}

#[inline]
fn to_path_string(path: &Path) -> String {
    path.to_str()
        .map(|s| s.to_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

// ── Integration tests ─────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_file(dir: &Path, name: &str, size: usize) {
        fs::write(dir.join(name), vec![0u8; size]).unwrap();
    }

    #[test]
    fn scan_empty_directory_returns_zero_size() {
        let tmp = TempDir::new().unwrap();
        let node = scan(tmp.path());
        assert!(node.is_dir);
        assert_eq!(node.size, 0);
        assert_eq!(node.file_count, 0);
        assert_eq!(node.dir_count, 0);
        assert!(node.error.is_none());
    }

    #[test]
    fn scan_directory_with_files_sums_sizes_correctly() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "a.txt", 1024);
        create_file(tmp.path(), "b.txt", 2048);
        let node = scan(tmp.path());
        assert_eq!(node.size, 1024 + 2048);
        assert_eq!(node.file_count, 2);
        assert_eq!(node.dir_count, 0);
    }

    #[test]
    fn scan_nested_directories_aggregates_recursively() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("subdir");
        fs::create_dir(&sub).unwrap();
        create_file(tmp.path(), "root.txt", 512);
        create_file(&sub, "nested.txt", 1024);
        let node = scan(tmp.path());
        assert_eq!(node.size, 512 + 1024);
        assert_eq!(node.file_count, 2);
        assert_eq!(node.dir_count, 1);
    }

    #[test]
    fn scan_children_are_sorted_largest_first() {
        let tmp = TempDir::new().unwrap();
        create_file(tmp.path(), "small.txt",  100);
        create_file(tmp.path(), "large.txt",  9000);
        create_file(tmp.path(), "medium.txt", 500);
        let node = scan(tmp.path());
        let sizes: Vec<u64> = node.children.iter().map(|c| c.size).collect();
        assert_eq!(sizes, vec![9000, 500, 100]);
    }

    #[test]
    fn scan_records_dir_name_from_path() {
        let tmp = TempDir::new().unwrap();
        let node = scan(tmp.path());
        assert!(!node.name.is_empty());
    }
}
