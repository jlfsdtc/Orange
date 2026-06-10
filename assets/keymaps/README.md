# Orange Default Keymap

Orange ships three platform-specific default keymaps. Most shortcuts use
GPUI's `secondary-` modifier, which resolves to `cmd` on macOS and `ctrl`
on Linux/Windows — so the same line covers both. A handful of bindings
differ between platforms to match native UI conventions; those are
called out below.

## Shared bindings (every platform)

| Action               | Default key                          | Description                                  |
| -------------------- | ------------------------------------ | -------------------------------------------- |
| `OpenFile`           | `secondary-o`                        | Open a log file in a new tab                 |
| `CloseTab`           | `secondary-w`                        | Close the active tab                         |
| `ScrollToTop`        | `secondary-home`                     | Jump to the first line                       |
| `ScrollToBottom`     | `secondary-end`                      | Jump to the last line                        |
| `PageUp`             | `pageup`                             | Scroll up by one page                        |
| `PageDown`           | `pagedown`                           | Scroll down by one page                      |
| `LineUp`             | `up`                                 | Move selection up one line                   |
| `LineDown`           | `down`                               | Move selection down one line                 |
| `GoToLineDialog`     | `secondary-l`                        | Open the "Go to line" dialog                 |
| `ToggleQuickFind`    | _(unbound)_                          | Show/hide the quick-find bar (opens automatically when a file loads; assign a key in Preferences to toggle it) |
| `FindNext`           | `f3` / `secondary-g`                 | Jump to the next match                       |
| `FindPrevious`       | `shift-f3` / `secondary-shift-g`     | Jump to the previous match                   |
| `CloseFind`          | _(unbound)_                          | Close the quick-find bar (no default key — `escape` is reserved for modal dialogs; assign a key in Preferences) |
| `ToggleFilterPanel`  | `secondary-shift-p`                  | Show/hide the predefined-filters panel       |
| `ToggleTailMode`     | `secondary-t`                        | Toggle tail mode (auto-refresh)              |
| `ToggleTheme`        | `secondary-shift-t`                  | Flip between light and dark theme            |
| `ToggleScratchpad`   | `secondary-shift-s`                  | Show/hide the scratchpad panel               |
| `SaveSession`        | `secondary-shift-d`                  | Open session manager (save)                  |
| `LoadSession`        | `secondary-shift-o`                  | Open session manager (load)                  |
| `OpenOptions`        | `secondary-,`                        | Open the application options dialog          |
| `IncreaseFontSize`   | `secondary-=` / `secondary-shift-=`  | Increase log-text font size by 1pt           |
| `DecreaseFontSize`   | `secondary--`                        | Decrease log-text font size by 1pt           |
| `ResetFontSize`      | `secondary-0`                        | Reset log-text font size to the default      |
| `CopySelection`      | `secondary-c`                        | Copy the selected log lines to the clipboard |
| `SelectTabN` (1–9)   | `secondary-1` … `secondary-9`        | Jump to tab N                                |
| `Quit`               | `secondary-q` (built-in)             | Quit Orange — also `alt-f4` on Linux/Windows |

## Platform-specific overrides

| Action               | macOS               | Linux            | Windows          | Why                                                                |
| -------------------- | ------------------- | ---------------- | ---------------- | ------------------------------------------------------------------ |
| `ToggleFilteredView` | `secondary-shift-r` | `f4`             | `f4`             | `ctrl-shift-r` is browser hard-reload muscle memory                |
| `CloseTab`           | `secondary-w`       | + `ctrl-f4`      | + `ctrl-f4`      | Classic MDI close-tab shortcut on Win/Linux desktops               |
| `Quit`               | `secondary-q`       | + `alt-f4`       | + `alt-f4`       | System-standard window close on Windows; common on Linux too       |

## How `secondary-` resolves

| Platform        | `secondary-` |
| --------------- | ------------ |
| macOS           | `cmd-`       |
| Linux / Windows | `ctrl-`      |

So `secondary-f` is `cmd-f` on macOS and `ctrl-f` on Linux/Windows.

## Preferences

`secondary-,` (Cmd+,/Ctrl+,) opens **Preferences**. On Linux/Windows the
shortcut is unusual for native desktop apps, so the application menu has
an explicit **Orange → Preferences…** item that fires the same action —
reach it via the menubar (or Alt to surface it on platforms with a hidden
menu) when the shortcut feels foreign.

## Customizing

The first time Orange launches it writes the platform default keymap to
`~/.orange/keymap.json` (the user's home directory on all platforms).

You can edit shortcuts two ways:

1. **In-app** — open **Preferences** (`secondary-,`) and use the **Shortcuts**
   section. Click a row to enter capture mode ("Press a key…"), then press
   the new combination — `Escape` cancels, `Backspace` unbinds the action.
   Saving applies the new bindings
   immediately, no restart required. If the new key was already bound to
   another action, the old action is unbound and a notice is shown.
2. **Edit the JSON file** directly and restart Orange. Action names match
   the tables above. Key strings follow GPUI's `KeyBinding::new` syntax
   (e.g. `"ctrl-shift-t"`, `"pageup"`, `"escape"`).

`default-macos.json`, `default-linux.json`, and `default-windows.json` in
this directory are reference copies of each platform's shipped defaults.
They are **not** read at runtime — they exist so packagers and users can
diff against their customizations.
