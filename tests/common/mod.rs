#![allow(dead_code)]

use assert_cmd::Command;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

pub struct TestDir {
    pub dir: TempDir,
}

impl TestDir {
    pub fn new() -> Self {
        Self {
            dir: TempDir::new().expect("failed to create tempdir"),
        }
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn write_config(&self, contents: &str) {
        fs::write(self.path().join("req.toml"), contents)
            .expect("failed to write req.toml");
    }

    pub fn write_file(&self, name: &str, contents: &str) {
        debug_assert!(
            !name.contains('/') && !name.contains('\\'),
            "name must be a bare filename, got {name:?}"
        );
        fs::write(self.path().join(name), contents)
            .expect("failed to write file");
    }
}

impl Default for TestDir {
    fn default() -> Self {
        Self::new()
    }
}

/// Build an `assert_cmd::Command` for the `req` binary.
///
/// - `current_dir` is set to `dir.path()` so cwd-relative resolution
///   (`.env`, multipart `file = "..."`) targets the test's tempdir.
/// - `env_clear()` strips parent process env so host `HTTP_PROXY`,
///   `.env`-derived vars, etc. cannot leak into the test.
/// - `PATH` is restored from the parent because some platforms need it
///   for runtime linker / TLS init paths.
/// - `HOME` is intentionally NOT restored: `reqwest` uses `rustls` with
///   bundled CA roots (no `~/.local/share/ca-certificates` lookup), and
///   `dotenvy` reads explicit relative paths (no `HOME` lookup).
pub fn req_command(dir: &TestDir) -> Command {
    let mut cmd = Command::cargo_bin("req").expect("req binary not built");
    cmd.current_dir(dir.path()).env_clear();
    if let Ok(path) = std::env::var("PATH") {
        cmd.env("PATH", path);
    }
    cmd
}
