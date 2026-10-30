//! Helpers shared by the unit tests

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A file in the temp directory, removed when dropped, also when the test fails
pub struct TempFile {
    pub path: PathBuf,
}

impl TempFile {
    pub fn new(name: &str, content: &str) -> Self {
        let path = temp_path(name);
        fs::write(&path, content).unwrap();

        Self { path }
    }

    pub fn read(&self) -> String {
        fs::read_to_string(&self.path).unwrap()
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// A directory in the temp directory, removed with what it holds when dropped
pub struct TempDir {
    pub path: PathBuf,
}

impl TempDir {
    pub fn new(name: &str) -> Self {
        let path = temp_path(name);
        fs::create_dir(&path).unwrap();

        Self { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// The name is for finding a file left behind, a number keeps the paths of tests apart
fn temp_path(name: &str) -> PathBuf {
    static COUNT: AtomicUsize = AtomicUsize::new(0);
    let number = COUNT.fetch_add(1, Ordering::Relaxed);

    env::temp_dir().join(format!("istoria-test-{name}-{}-{number}", process::id()))
}
