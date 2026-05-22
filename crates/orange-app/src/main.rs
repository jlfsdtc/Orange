//! orange: A fast log file viewer.
//!
//! Usage: orange [file] [--line N]

mod menus;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use clap::Parser;
use gpui::*;
use orange_settings::Keymap;
use orange_ui::keymap::{ApplyKeymap, build_key_bindings};
use orange_ui::main_window::MainWindowState;

/// Orange - a fast log file viewer
#[derive(Parser)]
#[command(name = "orange", version, about)]
struct Args {
    /// File to open
    file: Option<String>,

    /// Line number to scroll to
    #[arg(short, long)]
    line: Option<u64>,
}

/// Coordinates Finder "Open With" requests (`application:openURLs:` on macOS)
/// with the GPUI app. The platform callback runs on the main thread but doesn't
/// receive an `App`, so we stash an `AsyncApp` and the window handle here once
/// they exist, and buffer paths that arrive before the app is ready.
#[derive(Default)]
struct OpenRequest {
    pending: Vec<PathBuf>,
    cx: Option<AsyncApp>,
    window: Option<WindowHandle<MainWindowState>>,
}

fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let args = Args::parse();

    // Seed the default keymap on first run so users have an editable file.
    if let Err(e) = Keymap::write_default_if_missing() {
        tracing::warn!("could not seed default keymap: {e}");
    }

    let app = Application::new();
    let open_state: Rc<RefCell<OpenRequest>> = Rc::default();

    // Re-open the main window when the user clicks the dock icon after
    // closing every window (macOS only; on other platforms closing the
    // last window already quits the app).
    app.on_reopen(|cx| {
        open_main_window(cx, &[]);
    });

    // macOS sends files opened via Finder ("Open With", drag-onto-dock,
    // double-click on registered types) through `application:openURLs:`.
    // The callback fires on the main thread but without an `App`, so we
    // bounce work onto `AsyncApp` once it's available.
    {
        let open_state = open_state.clone();
        app.on_open_urls(move |urls| {
            let paths: Vec<PathBuf> = urls
                .iter()
                .filter_map(|u| file_url_to_path(u))
                .filter(|p| p.exists())
                .collect();
            if paths.is_empty() {
                return;
            }

            let cx_opt = open_state.borrow().cx.clone();
            match cx_opt {
                None => open_state.borrow_mut().pending.extend(paths),
                Some(cx) => {
                    let open_state = open_state.clone();
                    let _ = cx.update(move |cx| deliver_paths(&open_state, paths, cx));
                }
            }
        });
    }

    app.run(move |cx: &mut App| {
        apply_keymap(cx);

        // Register the application menu bar (macOS top-of-screen menu, plus
        // menu items on platforms that surface them per-window).
        cx.set_menus(menus::app_menus());

        // Quit menu handler.
        cx.on_action(|_: &menus::Quit, cx| cx.quit());
        // Re-apply the on-disk keymap after the in-app editor saves it.
        // `OptionsDialog` writes the file then dispatches `ApplyKeymap` so the
        // new bindings take effect without a restart.
        cx.on_action(|_: &ApplyKeymap, cx| apply_keymap(cx));

        // Publish the AsyncApp so subsequent openURL callbacks can dispatch.
        open_state.borrow_mut().cx = Some(cx.to_async());

        // Combine CLI arg with anything openURLs delivered during boot.
        let mut initial: Vec<PathBuf> = args
            .file
            .as_ref()
            .map(PathBuf::from)
            .filter(|p| p.exists())
            .into_iter()
            .collect();
        initial.append(&mut open_state.borrow_mut().pending);

        let window = open_main_window(cx, &initial);
        open_state.borrow_mut().window = Some(window);
    });
}

/// Replace every key binding with what's currently on disk, plus the
/// app-level Quit shortcut. Used both at startup and after the in-app keymap
/// editor saves changes — `cx.clear_key_bindings()` wipes the table including
/// the Quit binding, so we always re-add it here.
fn apply_keymap(cx: &mut App) {
    let keymap = Keymap::load_or_default();
    cx.clear_key_bindings();
    cx.bind_keys(build_key_bindings(&keymap.bindings));
    // `secondary-q` becomes Cmd+Q on macOS and Ctrl+Q on Linux/Windows;
    // non-mac platforms also accept the system-standard Alt+F4.
    cx.bind_keys([KeyBinding::new("secondary-q", menus::Quit, None)]);
    #[cfg(not(target_os = "macos"))]
    cx.bind_keys([KeyBinding::new("alt-f4", menus::Quit, None)]);
}

/// Open `paths` in the existing main window if it's still alive, otherwise
/// create one. Driven by the openURL callback after the app is running.
fn deliver_paths(open_state: &Rc<RefCell<OpenRequest>>, paths: Vec<PathBuf>, cx: &mut App) {
    if let Some(handle) = open_state.borrow().window {
        let added = handle
            .update(cx, |state, _window, cx| {
                for path in &paths {
                    state.open_file_from_path(path, cx);
                }
            })
            .is_ok();
        if added {
            handle
                .update(cx, |_, window, _| window.activate_window())
                .ok();
            return;
        }
    }
    let window = open_main_window(cx, &paths);
    open_state.borrow_mut().window = Some(window);
}

fn open_main_window(cx: &mut App, initial_files: &[PathBuf]) -> WindowHandle<MainWindowState> {
    let initial_files = initial_files.to_vec();
    let window = cx
        .open_window(
            WindowOptions {
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some("Orange".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| {
                let state = cx.new(|cx| MainWindowState::new(window, cx));

                // Give the root view keyboard focus so GPUI's action
                // dispatcher has a focus chain to walk — without this,
                // none of the global shortcuts (Cmd-O, Cmd-F, …) fire.
                let handle = state.read(cx).focus_handle(cx);
                window.focus(&handle);

                for path in &initial_files {
                    if path.exists() {
                        state.update(cx, |view, cx| {
                            view.open_file_from_path(path, cx);
                        });
                    }
                }

                state
            },
        )
        .unwrap();

    window
        .update(cx, |_, window, _| {
            window.activate_window();
        })
        .ok();

    window
}

/// Parse a `file://` URL into a local path, percent-decoding `%XX` escapes.
/// Returns `None` for non-file URLs or malformed encodings.
fn file_url_to_path(url: &str) -> Option<PathBuf> {
    let rest = url.strip_prefix("file://")?;
    // After "file://" the next "/" begins the absolute path; anything before
    // that is an (optional, usually empty) host segment.
    let path_start = rest.find('/')?;
    percent_decode(&rest[path_start..]).map(PathBuf::from)
}

fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16)?;
            let lo = (bytes[i + 2] as char).to_digit(16)?;
            out.push((hi * 16 + lo) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}
