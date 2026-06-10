# Orange

A fast log file viewer, written in Rust. Orange is a from-scratch rewrite of
[klogg](https://github.com/variar/klogg), targeting large log files with
regex filtering, live tail, bookmarks, and predefined highlight rules.

- **GUI** built on [GPUI](https://www.gpui.rs/) — runs natively on macOS, Linux, and Windows.
- **Search engine** backed by [Hyperscan](https://www.hyperscan.io/) and
  [roaring bitmaps](https://roaringbitmap.org/) for compact result storage.
- **Parallel indexing** with `rayon`, SIMD-accelerated UTF-8 validation, and
  compressed line-position storage so multi-GB logs stay responsive.

> Status: **0.1.0**. The GUI is usable, and `orange-grep` is a working
> headless search tool (boolean-expression mode is the one piece still
> unimplemented).

---

## Features

- Open arbitrarily large log files with virtual scrolling.
- Multiple files open at once as tabs (`Cmd/Ctrl-1`…`9` to switch).
- Filtered view that re-runs as you type, powered by Hyperscan regex.
- Quick-find bar (`Cmd/Ctrl-F`) with next/previous navigation and an editable
  search box: a focus-gated blinking caret you can position by click or with
  the arrow / Home / End keys, with text inserted at the caret.
- Predefined filter sets with persistent highlighting.
- Tail mode that follows file appends in real time (`notify`-based).
- Session save/restore — re-open the same files, filters, and scroll position.
- Side overview/minimap, a horizontal scroll bar for long lines, and
  per-line bookmarks.
- Copy selected lines, and adjustable log-text font size.
- A scratchpad panel for jotting notes alongside the log.
- Light and dark themes (`assets/themes/{light,dark}.json`).
- User-editable keymap that resolves `secondary-` to `cmd` on macOS and
  `ctrl` elsewhere — editable in-app or via JSON; see
  [`assets/keymaps/README.md`](assets/keymaps/README.md).

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
| Find next / previous | `⌘G` / `⌘⇧G` | `F3` / `Shift-F3` |
| Go to line           | `⌘L`   | `Ctrl-L`         |
| Toggle tail mode     | `⌘T`   | `Ctrl-T`         |
| Toggle filtered view | `⌘⇧R`  | `F4`             |
| Toggle filter panel  | `⌘⇧P`  | `Ctrl-Shift-P`   |
| Toggle minimap       | `⌘⇧M`  | `Ctrl-Shift-M`   |
| Toggle scratchpad    | `⌘⇧S`  | `Ctrl-Shift-S`   |
| Toggle theme         | `⌘⇧T`  | `Ctrl-Shift-T`   |
| Copy selection       | `⌘C`   | `Ctrl-C`         |
| Switch to tab 1–9    | `⌘1`–`⌘9` | `Ctrl-1`–`Ctrl-9` |
| Close tab            | `⌘W`   | `Ctrl-W`         |
| Open options         | `⌘,`   | `Ctrl-,`         |
| Quit                 | `⌘Q`   | `Ctrl-Q`         |

The full list — including font sizing and session shortcuts, and the
handful of per-platform overrides — lives in
[`assets/keymaps/README.md`](assets/keymaps/README.md).

### CLI (`orange-grep`)

Reuses the core engine (`orange_core::LogData` + `orange_regex::RegexEngine`)
for headless, grep-style search:

```sh
orange-grep [OPTIONS] <PATTERN> <FILE>
```

| Flag                 | Description                                    |
| -------------------- | ---------------------------------------------- |
| `<PATTERN>`          | Hyperscan regex to match                       |
| `<FILE>`             | Log file to search                             |
| `-i, --ignore-case`  | Case-insensitive search                        |
| `-n, --line-number`  | Prefix matches with their 1-based line number  |
| `-b, --boolean`      | Boolean-expression mode (not yet implemented)  |

---

## Configuration

Orange writes user state under `~/.orange/` (the user's home directory on
all platforms).

Files:

- `keymap.json` — written on first launch. Remap keys in-app via
  **Preferences → Shortcuts** (applied immediately) or by editing this file
  and restarting.
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
