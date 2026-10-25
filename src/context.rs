//! Error messages that say what failed

use std::fmt::Display;

/// Prefixes an error with what failed: `cannot delete: <error>`
pub trait Context<T> {
    fn context(self, prefix: impl Display) -> Result<T, String>;
}

impl<T, E: Display> Context<T> for Result<T, E> {
    fn context(self, prefix: impl Display) -> Result<T, String> {
        self.map_err(|error| format!("{prefix}: {error}"))
    }
}
