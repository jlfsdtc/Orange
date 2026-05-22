//! File change monitoring for live log viewing (tail mode).
//!
//! Uses the `notify` crate to watch for filesystem events on a log file.
//! Events are delivered via a channel for the UI to consume.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use notify::Watcher;

/// Events emitted by the file watcher.
#[derive(Debug, Clone)]
pub enum FileEvent {
    /// The file was modified (content changed).
    Modified(PathBuf),
    /// The file was created (new file at watched path).
    Created(PathBuf),
    /// The file was deleted.
    Deleted(PathBuf),
    /// The file was renamed.
    Renamed { from: PathBuf, to: PathBuf },
}

/// Watches a file for changes and delivers events via a channel.
pub struct FileWatcher {
    _watcher: notify::RecommendedWatcher,
    rx: mpsc::Receiver<notify::Result<notify::Event>>,
}

impl FileWatcher {
    /// Start watching a file for changes.
    pub fn watch(path: &Path) -> anyhow::Result<Self> {
        let (tx, rx) = mpsc::channel();

        let mut watcher = notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        })?;

        watcher.watch(path, notify::RecursiveMode::NonRecursive)?;

        Ok(Self {
            _watcher: watcher,
            rx,
        })
    }

    /// Poll for pending events (non-blocking).
    /// Returns all events that have accumulated since the last poll.
    pub fn poll_events(&self) -> Vec<FileEvent> {
        let mut events = Vec::new();

        while let Ok(result) = self.rx.try_recv() {
            if let Ok(notify_event) = result {
                if let Some(file_event) = Self::convert_event(notify_event) {
                    events.push(file_event);
                }
            }
        }

        events
    }

    /// Wait for the next event with a timeout (blocking).
    pub fn wait_event(&self, timeout: Duration) -> Option<FileEvent> {
        match self.rx.recv_timeout(timeout) {
            Ok(Ok(notify_event)) => Self::convert_event(notify_event),
            _ => None,
        }
    }

    /// Drain all pending events and return them.
    /// Alias for poll_events for clarity.
    pub fn drain(&self) -> Vec<FileEvent> {
        self.poll_events()
    }

    fn convert_event(event: notify::Event) -> Option<FileEvent> {
        use notify::EventKind;

        match event.kind {
            EventKind::Modify(_) => {
                event.paths.into_iter().next().map(FileEvent::Modified)
            }
            EventKind::Create(_) => {
                event.paths.into_iter().next().map(FileEvent::Created)
            }
            EventKind::Remove(_) => {
                event.paths.into_iter().next().map(FileEvent::Deleted)
            }
            EventKind::Other => {
                // Some platforms emit Other for renames
                if event.paths.len() >= 2 {
                    Some(FileEvent::Renamed {
                        from: event.paths[0].clone(),
                        to: event.paths[1].clone(),
                    })
                } else {
                    event.paths.into_iter().next().map(FileEvent::Modified)
                }
            }
            _ => event.paths.into_iter().next().map(FileEvent::Modified),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::NamedTempFile;

    #[test]
    fn test_watch_file_modification() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.log");
        fs::write(&path, b"initial content\n").unwrap();

        let watcher = FileWatcher::watch(&path).unwrap();

        // Modify the file
        fs::write(&path, b"initial content\nadded line\n").unwrap();

        // Wait for the event with timeout (macOS FSEvents can be delayed)
        let event = watcher.wait_event(Duration::from_secs(3));
        assert!(
            event.is_some(),
            "Expected a file event within 3 seconds for path: {:?}",
            path
        );
    }

    #[test]
    fn test_watch_no_events() {
        let file = NamedTempFile::new().unwrap();
        let watcher = FileWatcher::watch(file.path()).unwrap();

        // No modifications, should be empty
        std::thread::sleep(Duration::from_millis(100));
        let events = watcher.poll_events();
        assert!(events.is_empty());
    }

    #[test]
    fn test_wait_event_timeout() {
        let file = NamedTempFile::new().unwrap();
        let watcher = FileWatcher::watch(file.path()).unwrap();

        // Should timeout with no event
        let event = watcher.wait_event(Duration::from_millis(100));
        assert!(event.is_none());
    }
}
