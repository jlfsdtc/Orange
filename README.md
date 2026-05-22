# Orange

A fast log file viewer, written in Rust. Orange is a from-scratch rewrite of
[klogg](https://github.com/variar/klogg), targeting large log files with
regex filtering, live tail, bookmarks, and predefined highlight rules.

- **GUI** built on [GPUI](https://www.gpui.rs/) — runs natively on macOS, Linux, and Windows.
- **Search engine** backed by [Hyperscan](https://www.hyperscan.io/) and
  [roaring bitmaps](https://roaringbitmap.org/) for compact result storage.
- **Parallel indexing** with `rayon`, SIMD-accelerated UTF-8 validation, and
  compressed line-position storage so multi-GB logs stay responsive.

> Status: **0.1.0**. The GUI is usable; the `orange-grep` CLI is a stub.

---

## Features

- Open arbitrarily large log files with virtual scrolling.
- Filtered view that re-runs as you type, powered by Hyperscan regex.
- Quick-find bar (`Cmd/Ctrl-F`) with next/previous navigation.
- Predefined filter sets with persistent highlighting.
- Tail mode that follows file appends in real time (`notify`-based).
- Session save/restore — re-open the same files, filters, and scroll position.
- Side overview/minimap and per-line bookmarks.
- Light and dark themes (`assets/themes/{light,dark}.json`).
- User-editable keymap that resolves `secondary-` to `cmd` on macOS and
  `ctrl` elsewhere — see [`assets/keymaps/README.md`](assets/keymaps/README.md).

---

## Install

### Prebuilt packages

Releases are published via the [`release` workflow](.github/workflows/release.yml)
when a `v*` tag is pushed. Artifacts produced per platform:

| Platform | Artifact                          |
| -------- | --------------------------------- |
| macOS    | `orange.app`, `orange-<ver>.dmg`  |
| Linux    | `orange_<ver>_<arch>.deb`, RPM, AppImage |
| Windows  | NSIS installer (`installer/windows/orange.nsi`) |

The macOS bundle is **unsigned** — first launch requires right-click → Open
or `xattr -dr com.apple.quarantine /Applications/orange.app`.

### From source

Requirements:

- Rust **1.75+** (workspace `rust-version`)
- A C/C++ toolchain (Hyperscan builds a native library)
- macOS: Xcode command-line tools
- Linux: `build-essential`, `cmake`, `libboost-dev`, `ragel` (Hyperscan deps)
- Windows: MSVC build tools

```sh
git clone https://github.com/jlfsdtc/Orange.git
cd Orange
cargo build --release --bin orange
./target/release/orange path/to/log.txt
```

---

## Usage

### GUI

```sh
orange [FILE] [--line N]
```

| Flag           | Description                          |
| -------------- | ------------------------------------ |
| `FILE`         | Log file to open on startup          |
| `-l, --line N` | Scroll to line `N` after loading     |

Keyboard shortcuts are listed in [`assets/keymaps/README.md`](assets/keymaps/README.md);
the most common are:

| Action               | macOS  | Linux / Windows  |
| -------------------- | ------ | ---------------- |
| Open file            | `⌘O`   | `Ctrl-O`         |
| Quick find           | `⌘F`   | `Ctrl-F`         |
| Go to line           | `⌘L`   | `Ctrl-L`         |
| Toggle tail mode     | `⌘T`   | `Ctrl-T`         |
| Toggle filtered view | `⌘⇧R`  | `Ctrl-Shift-R`   |
| Toggle theme         | `⌘⇧T`  | `Ctrl-Shift-T`   |
| Open options         | `⌘,`   | `Ctrl-,`         |
| Quit                 | `⌘Q`   | `Ctrl-Q`         |

### CLI (`orange-grep`)

Reuses the core search engine for headless grep-style use. Currently a
stub — tracked in `crates/orange-app/src/grep.rs`.

---

## Configuration

Orange writes user state under the platform's standard config directory:

| Platform | Path                                            |
| -------- | ----------------------------------------------- |
| macOS    | `~/Library/Application Support/Orange/`         |
| Linux    | `~/.config/orange/`                             |
| Windows  | `%APPDATA%\Orange\`                             |

Files:

- `keymap.json` — written on first launch, edit and restart to remap keys.
- `settings.json` — UI/theme preferences.
- `sessions/` — saved session snapshots.

---

## Architecture

Orange is a Cargo workspace. Layering goes top-down: `orange-app` → `orange-ui`
→ `orange-core` → `orange-regex` / `orange-utils` / `orange-settings`.

| Crate              | Responsibility                                                      |
| ------------------ | ------------------------------------------------------------------- |
| `orange-app`       | Binary entry points (`orange`, `orange-grep`), menus, CLI parsing.  |
| `orange-ui`        | GPUI views: main window, log view, filtered view, dialogs, themes.  |
| `orange-core`      | File indexing, compressed line storage, encoding detection, tail.   |
| `orange-regex`     | Hyperscan-backed regex + boolean-expression compilation.            |
| `orange-settings`  | Config schema, keymap, persisted session format.                    |
| `orange-utils`     | Shared primitives (allocator, hashing, small helpers).              |

---

## Development

```sh
# Build everything
cargo build

# Run the GUI in debug mode
cargo run --bin orange -- path/to/log.txt

# Tests
cargo test --workspace

# Benchmarks (Criterion)
cargo bench -p orange-core
cargo bench -p orange-regex

# Lints
cargo clippy --workspace --all-targets
```

Logging is controlled by `RUST_LOG` (e.g. `RUST_LOG=orange_core=debug`).

### Packaging scripts

| Script                          | Output                                    |
| ------------------------------- | ----------------------------------------- |
| `scripts/package-macos.sh`      | `orange.app` + DMG                        |
| `scripts/package-deb.sh`        | Debian package via `cargo-deb`            |
| `scripts/package-rpm.sh`        | RPM via `cargo-generate-rpm`              |
| `scripts/package-appimage.sh`   | AppImage                                  |
| `scripts/package-windows.ps1`   | NSIS installer                            |
| `scripts/build-icons.py`        | Regenerate PNG icons from source SVG      |

---

## License

GPL-3.0-or-later. See [`LICENSE`](LICENSE). Orange is a Rust port and reuses
design choices from klogg, which is also GPL-licensed.
