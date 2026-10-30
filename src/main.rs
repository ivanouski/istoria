//! istoria, a finder for bash history: prints the command picked from $HISTFILE or its backups,
//! which it saves in SQLite

mod app;
mod backup;
mod cli;
mod config;
mod context;
mod finder;
mod history;
mod pane;
mod prompt;
#[cfg(test)]
mod test_util;
mod tty;
mod ui;

use std::env;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use app::{Outcome, Source};
use backup::Backups;
use chrono::Utc;
use cli::{Args, Search, Searched};
use config::Config;
use context::Context;
use finder::Finder;
use history::History;

// Exit statuses .istoriarc acts on. The query's own status keeps an older istoria's failure from
// reading as an empty query.
const QUIT_STATUS: u8 = 1;
const ERROR_STATUS: u8 = 2;
const QUERY_STATUS: u8 = 3;
const EDIT_STATUS: u8 = 4;
const ABORT_STATUS: u8 = 130;

fn main() -> ExitCode {
    match run() {
        Ok(status) => status,
        Err(message) => {
            eprintln!("istoria: {message}");
            ExitCode::from(ERROR_STATUS)
        }
    }
}

fn run() -> Result<ExitCode, String> {
    match cli::parse_args(env::args_os().skip(1))? {
        Args::Search(search) => return run_search(search),
        Args::DeleteLast(count) => delete_last(count)?,
        Args::InitConfig => print_line(&Config::init()?.display().to_string())?,
        Args::Backup => back_up()?,
        Args::BackupList => list_backups()?,
        Args::BackupDelete(label) => delete_backup(&label)?,
        Args::Help => print(&cli::help())?,
        Args::Version => print_line(&format!("istoria {}", env!("CARGO_PKG_VERSION")))?,
    }

    Ok(ExitCode::SUCCESS)
}

fn run_search(search: Search) -> Result<ExitCode, String> {
    let config = Config::load()?;
    let mut source = match search.searched {
        Searched::History => Source::History(load_history()?),
        Searched::Backups => backup_source(None)?,
        Searched::Backup(label) => backup_source(Some(label))?,
    };
    let mut finder = Finder::new(source.entries(), search.query);
    let prompt = search.prompt.map(|text| prompt::parse(&text));

    let outcome =
        app::pick(&mut finder, &mut source, &config, prompt.as_ref()).context("terminal error")?;

    match outcome {
        Outcome::Picked(command) => {
            print_line(&command)?;
            Ok(ExitCode::SUCCESS)
        }
        Outcome::Edited(command) => {
            print_line(&command)?;
            Ok(ExitCode::from(EDIT_STATUS))
        }
        Outcome::Quit if search.print_query => {
            print_line(finder.query())?;
            Ok(ExitCode::from(QUERY_STATUS))
        }
        Outcome::Quit => Ok(ExitCode::from(QUIT_STATUS)),
        Outcome::Abort => Ok(ExitCode::from(ABORT_STATUS)),
        Outcome::Signaled(signal) => Ok(ExitCode::from(tty::signal_status(signal))),
    }
}

/// Saves the history as a backup, prints its label and how many entries it has and added
fn back_up() -> Result<(), String> {
    let history = load_history()?;
    let saved = Backups::open(&backup::path()?)?.save(
        history.entries(),
        &backup::host(),
        Utc::now().timestamp(),
    )?;

    print_line(&format!(
        "{}  {} entries, {} new",
        saved.label, saved.entries, saved.added
    ))
}

/// One backup per line, oldest first: its label, host and entries, apart by tabs
fn list_backups() -> Result<(), String> {
    let Some(backups) = Backups::open_existing(&backup::path()?)? else {
        return Ok(());
    };

    for listed in backups.list()? {
        print_line(&format!(
            "{}\t{}\t{}",
            listed.label, listed.host, listed.entries
        ))?;
    }

    Ok(())
}

fn delete_backup(label: &str) -> Result<(), String> {
    let (mut backups, started_at) = find_backup(label)?;

    backups.delete(started_at)
}

/// The backup with the label, or all of them, without entries before the first backup
fn backup_source(label: Option<String>) -> Result<Source, String> {
    let (backups, started_at) = match &label {
        Some(label) => {
            let (backups, started_at) = find_backup(label)?;
            (Some(backups), Some(started_at))
        }
        None => (Backups::open_existing(&backup::path()?)?, None),
    };
    let entries = match &backups {
        Some(backups) => backups.entries(started_at)?,
        None => Vec::new(),
    };

    Ok(Source::Backups {
        backups,
        entries,
        label,
        started_at,
    })
}

/// The database and the start of the backup with the label
fn find_backup(label: &str) -> Result<(Backups, i64), String> {
    let no_backup = || format!("no backup {label}");
    let backups = Backups::open_existing(&backup::path()?)?.ok_or_else(no_backup)?;
    let started_at = backups.find(label)?.ok_or_else(no_backup)?;

    Ok((backups, started_at))
}

/// Prints the deleted commands, oldest first. Fewer commands than asked for deletes them all.
fn delete_last(count: usize) -> Result<(), String> {
    let history = load_history()?;
    let entries = history.entries();
    let indices: Vec<usize> = (entries.len().saturating_sub(count)..entries.len()).collect();

    if indices.is_empty() {
        return Ok(());
    }

    history.delete(&indices).context("cannot delete")?;

    // The commands are already deleted, so a closed stdout is no reason to fail
    let mut stdout = io::stdout().lock();

    for &index in &indices {
        if writeln!(stdout, "{}", entries[index].original).is_err() {
            break;
        }
    }

    Ok(())
}

fn load_history() -> Result<History, String> {
    let path = history_path()?;

    History::load(&path).context(format!("cannot read {}", path.display()))
}

fn history_path() -> Result<PathBuf, String> {
    match env::var_os("HISTFILE").filter(|path| !path.is_empty()) {
        Some(path) => Ok(path.into()),
        None => env::home_dir()
            .map(|home| home.join(".bash_history"))
            .ok_or_else(|| "HISTFILE and HOME are not set".to_string()),
    }
}

fn print_line(text: &str) -> Result<(), String> {
    print(&format!("{text}\n"))
}

fn print(text: &str) -> Result<(), String> {
    write!(io::stdout(), "{text}").context("cannot print")
}
