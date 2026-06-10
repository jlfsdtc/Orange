# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Orange is a fast log file viewer — a Rust rewrite of [klogg](https://github.com/variar/klogg).
It is a GPUI desktop app (`orange`) plus a CLI grep tool (`orange-grep` in
`crates/orange-app/src/grep.rs`). Targets macOS, Linux, and Windows.

## Common commands

```sh
cargo build                                  # workspace debug build
cargo build --release --bin orange           # release GUI binary
cargo run --bin orange -- path/to/log.txt    # run GUI in debug
cargo test --workspace                       # all tests
cargo test -p orange-core <name>             # single test by name in one crate
cargo bench -p orange-core                   # Criterion benches (also -p orange-regex)
cargo clippy --workspace --all-targets       # lint
RUST_LOG=orange_core=debug cargo run --bin orange    # scoped logs
```

Release workflow (`.github/workflows/release.yml`) fires on `v*` tags and runs
the platform scripts in `scripts/` (`package-macos.sh`, `package-deb.sh`,
`package-rpm.sh`, `package-appimage.sh`, `package-windows.ps1`).

## Architecture

Cargo workspace, six crates, strict top-down dependency direction. Never
introduce upward edges (e.g. `orange-core` must not depend on `orange-ui`):

```
orange-app ──► orange-ui ──► orange-core ──► orange-regex
                  │              │           orange-utils
                  └──────────────┴──────────► orange-settings
```

- **`orange-app`** — binary entry points (`orange`, `orange-grep`), menu bar,
  `clap` CLI parsing, `tracing_subscriber` init. Owns the GPUI `Application`
  lifecycle, including macOS dock-icon re-open and the `Quit` action
  (bound at app level, not in the user keymap).
- **`orange-ui`** — all GPUI views: main window, log view (virtual scroll),
  filtered view, overview/minimap, quick-find, options dialog, scratchpad,
  session widget, predefined-filters panel, themes. Sets
  `#![recursion_limit = "1024"]` because GPUI's view-tree macros expand deeply.
- **`orange-core`** — performance-critical data layer: `LogData`,
  `LogFilteredData`, compressed line storage, parallel block indexing
  (`rayon`), encoding detection (`chardetng` + `simdutf8`), file tail
  (`notify`). Has a `build.rs` (`cc`) that compiles native helpers — a C/C++
  toolchain is required even for plain `cargo build`.
- **`orange-regex`** — Hyperscan-backed regex + boolean-expression engine.
  Hyperscan pulls in a sizeable native build (cmake, boost, ragel on Linux).
- **`orange-settings`** — config schema, persisted session format, keymap
  loader. `Keymap::write_default_if_missing()` runs at startup to seed an
  editable `keymap.json`; `Keymap::load_or_default()` reads it.
- **`orange-utils`** — shared primitives: the `mimalloc` global allocator,
  xxhash helpers, small utilities.

## Conventions to preserve

- **Keymap**: the user-editable keymap lives in `~/.orange/` (the user's
  home directory on all platforms). Action names and key syntax
  are documented in `assets/keymaps/README.md`; the three reference copies
  `assets/keymaps/default-{macos,linux,windows}.json` are **not** read at
  runtime — each must stay in sync with its sibling `default_bindings_*()`
  in `orange-settings/src/keymap.rs`, since `Keymap::write_default_if_missing()`
  picks the table by `cfg!(target_os = …)`.
- **`secondary-` modifier**: GPUI resolves it to `cmd` on macOS and `ctrl`
  elsewhere. Use it instead of hard-coding either; only the `Quit` action
  is bound outside the user keymap (in `orange-app`).
- **Themes** live in `assets/themes/{light,dark}.json` — not in code.
- **Quick-find input** (`orange-ui/src/quick_find.rs`) is a hand-rolled text
  field, not a GPUI text input: each query char is its own `div` so mouse
  hit-testing maps clicks to byte offsets. `caret` is the byte-offset insertion
  point (typing/backspace/arrows act there), drawn as a 1px bar that blinks via
  a `start_blink` background task (modeled on `log_view`'s `start_tail_poll`).
  The caret is painted only when `focus_handle.is_focused(window)` — clicking
  away blurs the bar and hides it. Keep `caret` on a char boundary when editing.
- **Release profile** (`Cargo.toml`) uses `lto = true`, `codegen-units = 1`,
  `strip = "symbols"`. Don't relax these without a reason; they're load-bearing
  for binary size and startup latency.
- **Packaging metadata** for `cargo-deb` and `cargo-generate-rpm` is encoded
  inside `crates/orange-app/Cargo.toml`. Asset paths there are relative to
  that crate's directory — easy to break when moving icons or desktop files.
- **macOS bundles are unsigned**. The packaging script documents the
  `xattr -dr com.apple.quarantine` workaround; don't claim signed/notarized
  output.

## Status notes

- Version is `0.1.0`. `orange-grep` works — it reuses `orange_core::LogData` +
  `orange_regex::RegexEngine` to do line-by-line regex matching (`-i`, `-n`
  flags). The only unimplemented piece is `--boolean` mode, which currently
  bails with an error.
- Repo: <https://github.com/jlfsdtc/Orange>.
