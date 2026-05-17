//! Shell adapter.
//!
//! Per ADR-0004 the only place in the codebase that talks to the OS
//! shell (open URLs, reveal files in Finder, open blobs in the default
//! viewer) is this module. Per ADR-0005 the trait-plus-fake split is
//! the load-bearing testability lever: `commands::open_link_with` and
//! `commands::reveal_capture_with` compose against `&dyn Shell` so
//! tests can assert which method got called with which argument
//! without spawning a real `open(1)` subprocess.
//!
//! The real backend (`SystemShell`) is macOS-specific and shells out
//! to `/usr/bin/open` for all three operations. We considered adding
//! `tauri-plugin-shell` for `open_in_browser` but it would have meant
//! pulling a new plugin in, registering it in `lib::run`, and adding a
//! `shell:allow-open` capability for one call site. Going through
//! `open(1)` directly keeps the dependency surface flat and the three
//! methods symmetric (one `Command::new("open")` shape, three arg
//! shapes). v1.0 is macOS-only per the PRD "Out of Scope" list.

use std::path::Path;
use std::process::Command;

#[derive(Debug)]
pub enum ShellError {
    /// Spawning `open(1)` (or its plugin equivalent) failed.
    Spawn(String),
}

impl std::fmt::Display for ShellError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShellError::Spawn(msg) => write!(f, "shell spawn error: {msg}"),
        }
    }
}

impl std::error::Error for ShellError {}

/// Adapter trait. One method per Open action. Each method returns
/// `Result<(), ShellError>` so callers can surface failure to the JS
/// layer; `SystemShell` only errors if the spawn itself fails (e.g.
/// `open` is missing from `PATH`), not if the spawned process later
/// reports a failure — we do not wait on the child.
pub trait Shell {
    fn open_in_browser(&self, url: &str) -> Result<(), ShellError>;
    fn reveal_in_finder(&self, path: &Path) -> Result<(), ShellError>;
    fn open_path(&self, path: &Path) -> Result<(), ShellError>;
}

/// Real backend. macOS `/usr/bin/open` dispatches by URL scheme
/// (`http://`, `https://`, `mailto:`, etc.) and by path; `-R` is the
/// "reveal in Finder" flag. We spawn-and-forget so the command returns
/// immediately and the user's app stays responsive.
pub struct SystemShell;

impl SystemShell {
    pub fn new() -> Self {
        SystemShell
    }
}

impl Default for SystemShell {
    fn default() -> Self {
        Self::new()
    }
}

impl Shell for SystemShell {
    fn open_in_browser(&self, url: &str) -> Result<(), ShellError> {
        Command::new("open")
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(|e| ShellError::Spawn(e.to_string()))
    }

    fn reveal_in_finder(&self, path: &Path) -> Result<(), ShellError> {
        Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|e| ShellError::Spawn(e.to_string()))
    }

    fn open_path(&self, path: &Path) -> Result<(), ShellError> {
        Command::new("open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|e| ShellError::Spawn(e.to_string()))
    }
}

/// Test-support fake. Lives at module scope (not `#[cfg(test)]`) so
/// integration tests under `src-tauri/tests/` can reach it, mirroring
/// the `FakeClipboard` shape. Records every call so tests can assert
/// the exact `(method, argument)` pair the helper produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellCall {
    OpenInBrowser(String),
    RevealInFinder(std::path::PathBuf),
    OpenPath(std::path::PathBuf),
}

pub struct FakeShell {
    calls: std::sync::Mutex<Vec<ShellCall>>,
}

impl FakeShell {
    pub fn new() -> Self {
        FakeShell {
            calls: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Snapshot the recorded calls in order.
    pub fn calls(&self) -> Vec<ShellCall> {
        self.calls.lock().expect("fake shell mutex poisoned").clone()
    }
}

impl Default for FakeShell {
    fn default() -> Self {
        Self::new()
    }
}

impl Shell for FakeShell {
    fn open_in_browser(&self, url: &str) -> Result<(), ShellError> {
        self.calls
            .lock()
            .expect("fake shell mutex poisoned")
            .push(ShellCall::OpenInBrowser(url.to_string()));
        Ok(())
    }

    fn reveal_in_finder(&self, path: &Path) -> Result<(), ShellError> {
        self.calls
            .lock()
            .expect("fake shell mutex poisoned")
            .push(ShellCall::RevealInFinder(path.to_path_buf()));
        Ok(())
    }

    fn open_path(&self, path: &Path) -> Result<(), ShellError> {
        self.calls
            .lock()
            .expect("fake shell mutex poisoned")
            .push(ShellCall::OpenPath(path.to_path_buf()));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn fake_shell_records_open_in_browser() {
        let fake = FakeShell::new();
        fake.open_in_browser("https://example.com").expect("ok");
        assert_eq!(
            fake.calls(),
            vec![ShellCall::OpenInBrowser("https://example.com".into())]
        );
    }

    #[test]
    fn fake_shell_records_reveal_in_finder() {
        let fake = FakeShell::new();
        fake.reveal_in_finder(Path::new("/tmp/x.pdf")).expect("ok");
        assert_eq!(
            fake.calls(),
            vec![ShellCall::RevealInFinder(PathBuf::from("/tmp/x.pdf"))]
        );
    }

    #[test]
    fn fake_shell_records_open_path() {
        let fake = FakeShell::new();
        fake.open_path(Path::new("/tmp/x.png")).expect("ok");
        assert_eq!(
            fake.calls(),
            vec![ShellCall::OpenPath(PathBuf::from("/tmp/x.png"))]
        );
    }
}
