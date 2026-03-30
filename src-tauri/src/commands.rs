use serde::Serialize;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tauri::{Emitter, WebviewWindow};

use crate::node::DirNode;
use crate::scanner;

// ── Event payloads ────────────────────────────────────────────────────────────

#[derive(Serialize, Clone)]
pub struct ScanStartPayload {
    pub path: String,
}

#[derive(Serialize, Clone)]
pub struct ScanProgressPayload {
    pub dirs_scanned: u64,
}

#[derive(Serialize, Clone)]
pub struct ScanCompletePayload {
    pub data:       DirNode,
    pub elapsed_ms: u64,
}

// ── Tauri commands ────────────────────────────────────────────────────────────

/// Starts a parallel disk scan in a background thread.
/// Emits:
///   - `scan:start`    — immediately
///   - `scan:progress` — every 200 ms with the current directory count
///   - `scan:complete` — when done, with the full tree and elapsed time
#[tauri::command]
pub fn start_scan(path: String, window: WebviewWindow) -> Result<(), String> {
    let root = PathBuf::from(&path);

    if !root.exists() {
        return Err(format!("Path not found: {path}"));
    }

    std::thread::spawn(move || {
        let start = std::time::Instant::now();
        scanner::reset_scan_counter();

        window
            .emit("scan:start", ScanStartPayload { path: path.clone() })
            .ok();

        // ── Progress reporter ─────────────────────────────────────────────────
        // Runs alongside the scan and emits progress every 200 ms.
        let done = Arc::new(AtomicBool::new(false));
        let done_clone = done.clone();
        let win_progress = window.clone();

        std::thread::spawn(move || {
            while !done_clone.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(200));
                win_progress
                    .emit(
                        "scan:progress",
                        ScanProgressPayload { dirs_scanned: scanner::dirs_scanned() },
                    )
                    .ok();
            }
        });

        // ── Scan ──────────────────────────────────────────────────────────────
        let scanned = scanner::scan(&root);
        done.store(true, Ordering::Relaxed); // stop the progress thread

        let elapsed_ms = start.elapsed().as_millis() as u64;

        // Limit serialisation depth to prevent stack-overflow / OOM when
        // sending 700 k+ nodes over the Tauri IPC as a single JSON payload.
        // Stats (size / dir_count / file_count) are preserved at every node
        // so the UI still shows correct totals; the user can drill in on
        // demand by clicking ⤵ on any truncated directory.
        let trimmed   = crate::node::trim_to_depth(scanned, 5);
        let root_node = wrap_in_drives_root(trimmed);

        window
            .emit("scan:complete", ScanCompletePayload { data: root_node, elapsed_ms })
            .ok();
    });

    Ok(())
}

/// Opens a path in the OS file explorer.
#[tauri::command]
pub fn open_path(path: String) -> Result<(), String> {
    let explorer = if cfg!(target_os = "windows")    { "explorer" }
                   else if cfg!(target_os = "macos") { "open" }
                   else                               { "xdg-open" };

    std::process::Command::new(explorer)
        .arg(&path)
        .spawn()
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Returns the list of available drives / mount-points on the current OS.
///
/// Windows : checks every letter A–Z for existence (e.g. `C:\`, `D:\`).
/// macOS   : lists entries under `/Volumes/`.
/// Linux   : returns `/` plus every direct child of `/mnt` and `/media`.
#[tauri::command]
pub fn list_drives() -> Vec<String> {
    #[cfg(target_os = "windows")]
    {
        (b'A'..=b'Z')
            .map(|c| format!("{}:\\", c as char))
            .filter(|d| std::path::Path::new(d).exists())
            .collect()
    }

    #[cfg(target_os = "macos")]
    {
        read_dir_paths("/Volumes").unwrap_or_else(|| vec!["/".to_owned()])
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let mut drives = vec!["/".to_owned()];
        for base in &["/mnt", "/media"] {
            if let Some(mut entries) = read_dir_paths(base) {
                drives.append(&mut entries);
            }
        }
        drives
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

#[cfg(not(target_os = "windows"))]
fn read_dir_paths(base: &str) -> Option<Vec<String>> {
    std::fs::read_dir(base).ok().map(|rd| {
        rd.filter_map(|e| e.ok())
            .map(|e| e.path().to_string_lossy().into_owned())
            .collect()
    })
}

fn wrap_in_drives_root(node: DirNode) -> DirNode {
    let mut root = DirNode::new_dir("Drives", "");
    root.size       = node.size;
    root.dir_count  = 1 + node.dir_count;
    root.file_count = node.file_count;
    root.children   = vec![node];
    root
}
