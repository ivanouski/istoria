//! Helpers shared by the unit tests

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process;
use std::sync::atomic::{AtomicUsize, Ordering};

use ratatui::style::{Color, Style};

use crate::config::{Config, Layout, ListPosition, SearchMode, Theme};
use crate::finder::{Command, Finder};
use crate::history::Entry;

/// Every key set, so the tests don't change with the defaults
pub fn config() -> Config {
    Config {
        height: 12,
        list: ListPosition::Below,
        layout: Layout::Recent,
        bar: '▪',
        border: false,
        head: 10,
        gap: '⋯',
        preview: false,
        times: true,
        search: SearchMode::Keyword,
        typos: true,
        colors: theme(),
    }
}

/// Every color set, so the tests don't change with the defaults
pub fn theme() -> Theme {
    Theme {
        prompt: Style::new().fg(Color::Blue).bold(),
        backup: Style::new().fg(Color::Yellow),
        count: Style::new().fg(Color::DarkGray),
        deleted: Style::new().fg(Color::Green),
        error: Style::new().fg(Color::Red),
        matched: Style::new().fg(Color::Yellow).bold(),
        dim: Style::new().fg(Color::DarkGray),
        selected: Style::new().bg(Color::DarkGray),
    }
}

/// Searching words without typos, most recent first, the matching most tests are not about
pub fn finder(entries: &[Entry], query: &str) -> Finder {
    Finder::new(
        entries,
        query.to_string(),
        Layout::Recent,
        SearchMode::Keyword,
        false,
    )
}

/// The heaviest first, typos when asked
pub fn weights_finder(entries: &[Entry], query: &str, typos: bool) -> Finder {
    Finder::new(
        entries,
        query.to_string(),
        Layout::Weights,
        SearchMode::Keyword,
        typos,
    )
}

/// A command of the finder, by its text
pub fn command<'a>(finder: &'a Finder, text: &str) -> &'a Command {
    finder
        .commands()
        .iter()
        .find(|command| command.text == text)
        .unwrap()
}

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
