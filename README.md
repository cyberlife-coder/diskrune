# 🪨 diskrune

**Fast parallel disk space analyzer for Windows, macOS and Linux.**
Built with Rust + Tauri 2, powered by Rayon work-stealing parallelism.

---

## Features

- **Ultra-fast scanning** — parallel recursive traversal across all CPU cores via Rayon
- **Live scan progress** — animated shimmer bar + real-time directory counter while scanning
- **Dynamic drive detection** — drive buttons populated automatically at startup (Windows: A–Z probe; macOS: `/Volumes`; Linux: `/`, `/mnt`, `/media`)
- **Interactive tree view** — expandable directory tree with size bars and percentages
- **Top 15 biggest directories** — instant ranking with colour-coded usage bars
- **Drill-down navigation** — click ⤵ on any folder to re-scan it as the new root, with breadcrumb history to navigate back
- **Full-text search** — find any folder by name or path in real time
- **Folder picker** — browse to any directory with the native OS dialog (📂)
- **Bounded IPC payload** — tree is trimmed to 5 levels before sending to the UI (stats preserved at every cut-off), preventing OOM on large drives like `C:\`
- **Syscall-efficient** — uses `file_type()` from `readdir` cache instead of a separate `stat()` per entry
- **Symlink-safe** — symlinks are skipped to avoid infinite loops and double-counting
- **Single-pass stats** — size, directory count and file count computed in one fold

---

## Screenshots

> Coming soon — run locally with `cargo tauri dev`

---

## Build

### Prerequisites

- [Rust](https://rustup.rs/) 1.77+
- [Tauri v2 CLI](https://v2.tauri.app/start/prerequisites/): `cargo install tauri-cli --version "^2"`
- **Windows** — WebView2 (included with Windows 11 / available as redistributable)
- **Linux** — GTK + WebKit system libraries:
  ```bash
  sudo apt install libgtk-3-dev libwebkit2gtk-4.1-dev \
    libappindicator3-dev librsvg2-dev patchelf
  ```

### Development

```bash
git clone https://github.com/cyberlife-coder/diskrune.git
cd diskrune
cargo tauri dev
```

### Release build

```bash
cargo tauri build
```

The installer is generated in `src-tauri/target/release/bundle/`.

---

## Architecture

```
src-tauri/src/
├── main.rs       Entry point (hides console window in release)
├── lib.rs        Tauri builder — registers commands and plugins
├── commands.rs   Tauri commands: start_scan, open_path, list_drives
├── scanner.rs    Parallel recursive scanner (Rayon) + AtomicU64 progress counter
└── node.rs       DirNode model, aggregate(), sort_by_size_desc(), trim_to_depth()
```

### Performance design

| Technique | Impact |
|-----------|--------|
| `rayon::par_iter()` recursive descent | All CPU cores used automatically |
| `file_type()` instead of `metadata()` for dirs | ~50% fewer syscalls on dir-heavy trees |
| Single-pass `aggregate()` fold | 3× fewer iterations over children |
| `AtomicU64` progress counter | Lock-free dir count across rayon threads |
| `trim_to_depth(node, 5)` before IPC emit | Prevents OOM on 700 k+ node trees |
| `sort_unstable_by` | Fastest in-place sort, no extra allocation |
| Release: `lto=true, codegen-units=1, opt-level=3` | Maximum compiler optimisation |

### IPC payload strategy

diskrune sends only the first 5 levels of the directory tree to the frontend.
Aggregated stats (total size, dir count, file count) are preserved at every
trimmed node so all numbers remain accurate.  Clicking ⤵ on any directory
triggers a fresh `start_scan` on that path, loading its subtree on demand.

---

## Ecosystem

diskrune is part of the **Veles** ecosystem by [Wiscale France](https://wiscale.fr).

---

## License

[MIT](LICENSE)
