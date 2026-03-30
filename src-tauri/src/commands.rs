use serde::Serialize;
use std::path::PathBuf;
use tauri::{Emitter, WebviewWindow};

use crate::node::DirNode;
use crate::scanner;

// ── Event payloads ────────────────────────────────────────────────────────────

#[derive(Serialize, Clone)]
pub struct ScanStartPayload {
    pub path: String,
}

#[derive(Serialize, Clone)]
pub struct ScanCompletePayload {
    pub data:       DirNode,
    pub elapsed_ms: u64,
}

// ── Tauri commands ────────────────────────────────────────────────────────────

/// Starts a parallel disk scan in a background thread.
/// Emits `scan:start` immediately, then `scan:complete` with the full tree.
#[tauri::command]
pub fn start_scan(path: String, window: WebviewWindow) -> Result<(), String> {
    let root = PathBuf::from(&path);

    if !root.exists() {
        return Err(format!("Path not found: {path}"));
    }

    std::thread::spawn(move || {
        let start = std::time::Instant::now();

        window
            .emit("scan:start", ScanStartPayload { path: path.clone() })
            .ok();

        let scanned = scanner::scan(&root);
        let elapsed_ms = start.elapsed().as_millis() as u64;

        // Wrap in a virtual "Drives" root — matches the viewer's expected shape.
        let root_node = wrap_in_drives_root(scanned);

        window
            .emit("scan:complete", ScanCompletePayload { data: root_node, elapsed_ms })
            .ok();
    });

    Ok(())
}

/// Opens a path in the OS file explorer.
#[tauri::command]
pub fn open_path(path: String) -> Result<(), String> {
    let explorer = if cfg!(target_os = "windows")      { "explorer" }
                   else if cfg!(target_os = "macos")   { "open" }
                   else                                  { "xdg-open" };

    std::process::Command::new(explorer)
        .arg(&path)
        .spawn()
        .map_err(|e| e.to_string())?;

    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn wrap_in_drives_root(node: DirNode) -> DirNode {
    let mut root = DirNode::new_dir("Drives", "");
    root.size       = node.size;
    root.dir_count  = node.dir_count;
    root.file_count = node.file_count;
    root.children   = vec![node];
    root
}
