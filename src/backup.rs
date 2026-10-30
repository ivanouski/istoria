//! Backups of the history in SQLite: the entries each backup saw, every entry stored once

use std::env;
use std::fs::{self, DirBuilder, OpenOptions};
use std::io;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local, NaiveDateTime, TimeZone};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

use crate::context::Context;
use crate::history::Entry;

mod schema;

/// The label of a backup, when it started in local time
const LABEL_FORMAT: &str = "%Y%m%d-%H%M%S";
const LABEL_WIDTH: usize = "20261030-142501".len();

pub struct Backups {
    connection: Connection,
}

/// What a new backup saved
#[derive(Debug, PartialEq)]
pub struct Saved {
    pub label: String,
    /// Distinct entries of the history
    pub entries: usize,
    /// Entries no backup had before
    pub added: usize,
}

#[derive(Debug, PartialEq)]
pub struct Listed {
    pub label: String,
    pub host: String,
    pub entries: i64,
}

impl Backups {
    /// Creates the database and its directory when missing, both only the owner can read, as
    /// history may hold secrets
    pub fn open(path: &Path) -> Result<Self, String> {
        let cannot_create = |error| format!("cannot create {}: {error}", path.display());

        if let Some(dir) = path.parent() {
            create_private_dir(dir).map_err(cannot_create)?;
        }

        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(path)
            .map_err(cannot_create)?;

        Self::connect(path)
    }

    /// None when there is no database yet, so reading creates nothing
    pub fn open_existing(path: &Path) -> Result<Option<Self>, String> {
        match fs::metadata(path) {
            Ok(_) => Self::connect(path).map(Some),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(format!("cannot read {}: {error}", path.display())),
        }
    }

    fn connect(path: &Path) -> Result<Self, String> {
        let connection =
            Connection::open(path).context(format!("cannot open {}", path.display()))?;

        Self::init(connection).context(path.display())
    }

    /// Foreign keys are off in every new connection, and the cascades need them
    fn init(connection: Connection) -> Result<Self, String> {
        connection
            .pragma_update(None, "foreign_keys", true)
            .map_err(|error| error.to_string())?;
        schema::prepare(&connection)?;
        Ok(Self { connection })
    }

    /// Saves the entries as a backup started at the time, adding those no backup had
    pub fn save(
        &mut self,
        entries: &[Entry],
        host: &str,
        started_at: i64,
    ) -> Result<Saved, String> {
        // The write lock up front, as one taken after the read can't wait for another backup
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("cannot back up")?;

        let exists = transaction
            .query_row(
                "SELECT 1 FROM backups WHERE started_at = ?1",
                [started_at],
                |_| Ok(()),
            )
            .optional()
            .context("cannot back up")?
            .is_some();

        if exists {
            return Err(format!(
                "backup {} already exists, try again",
                label(started_at)
            ));
        }

        let saved = insert(&transaction, entries, host, started_at).context("cannot back up")?;
        transaction.commit().context("cannot back up")?;

        Ok(saved)
    }

    /// Oldest first
    pub fn list(&self) -> Result<Vec<Listed>, String> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT backups.started_at, host, count(entry_id) FROM backups
                 LEFT JOIN entry_backups ON entry_backups.started_at = backups.started_at
                 GROUP BY backups.started_at ORDER BY backups.started_at",
            )
            .context("cannot list backups")?;

        statement
            .query_map([], |row| {
                Ok(Listed {
                    label: label(row.get(0)?),
                    host: row.get(1)?,
                    entries: row.get(2)?,
                })
            })
            .and_then(Iterator::collect)
            .context("cannot list backups")
    }

    /// The start of the backup with the label, None without one
    pub fn find(&self, label: &str) -> Result<Option<i64>, String> {
        let Some(started_at) = started_at(label) else {
            return Ok(None);
        };

        self.connection
            .query_row(
                "SELECT started_at FROM backups WHERE started_at = ?1",
                [started_at],
                |row| row.get(0),
            )
            .optional()
            .context("cannot read backups")
    }

    /// Deletes the backup and the entries no other backup has
    pub fn delete(&mut self, started_at: i64) -> Result<(), String> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("cannot delete the backup")?;

        transaction
            .execute("DELETE FROM backups WHERE started_at = ?1", [started_at])
            .and_then(|_| {
                transaction.execute(
                    "DELETE FROM entries WHERE NOT EXISTS
                     (SELECT 1 FROM entry_backups WHERE entry_id = entries.id)",
                    [],
                )
            })
            .and_then(|_| transaction.commit())
            .context("cannot delete the backup")
    }

    /// The entries of one backup or all of them, oldest first like a history file. Entries without
    /// a timestamp come first, in the order they were first saved.
    pub fn entries(&self, started_at: Option<i64>) -> Result<Vec<Entry>, String> {
        let entry = |row: &rusqlite::Row| -> rusqlite::Result<Entry> {
            let time: i64 = row.get(0)?;
            Ok(Entry::new(row.get(1)?, (time != 0).then_some(time)))
        };

        let entries = match started_at {
            None => self
                .connection
                .prepare("SELECT time, command FROM entries ORDER BY time, id")
                .and_then(|mut statement| statement.query_map([], entry)?.collect()),
            Some(started_at) => self
                .connection
                .prepare(
                    "SELECT time, command FROM entry_backups
                     JOIN entries ON entries.id = entry_id
                     WHERE started_at = ?1 ORDER BY time, id",
                )
                .and_then(|mut statement| statement.query_map([started_at], entry)?.collect()),
        };

        entries.context("cannot read backups")
    }
}

/// Saves the backup's row and its entries
fn insert(
    transaction: &Transaction,
    entries: &[Entry],
    host: &str,
    started_at: i64,
) -> rusqlite::Result<Saved> {
    transaction.execute(
        "INSERT INTO backups (started_at, host) VALUES (?1, ?2)",
        params![started_at, host],
    )?;

    let mut add_entry = transaction
        .prepare("INSERT INTO entries (time, command) VALUES (?1, ?2) ON CONFLICT DO NOTHING")?;
    let mut entry_id =
        transaction.prepare("SELECT id FROM entries WHERE time = ?1 AND command = ?2")?;
    let mut link = transaction.prepare(
        "INSERT INTO entry_backups (started_at, entry_id) VALUES (?1, ?2) ON CONFLICT DO NOTHING",
    )?;
    let mut saved = Saved {
        label: label(started_at),
        entries: 0,
        added: 0,
    };

    for entry in entries {
        let key = params![entry.time.unwrap_or(0), entry.original];
        saved.added += add_entry.execute(key)?;
        let id: i64 = entry_id.query_row(key, |row| row.get(0))?;
        // The same command at the same time twice is one entry
        saved.entries += link.execute(params![started_at, id])?;
    }

    Ok(saved)
}

/// Only the owner can read the directory, its parents are made as usual
fn create_private_dir(dir: &Path) -> io::Result<()> {
    if let Some(parent) = dir.parent() {
        fs::create_dir_all(parent)?;
    }

    match DirBuilder::new().mode(0o700).create(dir) {
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        result => result,
    }
}

/// `$XDG_DATA_HOME/istoria/history.db`, a relative `XDG_DATA_HOME` ignored as the XDG spec says
pub fn path() -> Result<PathBuf, String> {
    env::var_os("XDG_DATA_HOME")
        .filter(|dir| Path::new(dir).is_absolute())
        .map(PathBuf::from)
        .or_else(|| env::home_dir().map(|home| home.join(".local").join("share")))
        .map(|dir| dir.join("istoria").join("history.db"))
        .ok_or_else(|| "XDG_DATA_HOME and HOME are not set".to_string())
}

/// The host name up to the first dot
pub fn host() -> String {
    let uname = rustix::system::uname();
    let name = uname.nodename().to_string_lossy();

    name.split('.').next().unwrap_or_default().to_string()
}

/// When the backup started, in local time: `20261030-142501`
pub fn label(started_at: i64) -> String {
    DateTime::from_timestamp(started_at, 0).map_or_else(
        || started_at.to_string(),
        |time| time.with_timezone(&Local).format(LABEL_FORMAT).to_string(),
    )
}

/// The local time of a label, None when it isn't one
pub fn parse_label(label: &str) -> Option<NaiveDateTime> {
    let shape = label.len() == LABEL_WIDTH
        && label.bytes().enumerate().all(|(index, byte)| match index {
            8 => byte == b'-',
            _ => byte.is_ascii_digit(),
        });

    shape
        .then(|| NaiveDateTime::parse_from_str(label, LABEL_FORMAT).ok())
        .flatten()
}

/// The seconds of a label. A time the clocks went back over is the earlier one, a time they
/// skipped is none.
fn started_at(label: &str) -> Option<i64> {
    let time = parse_label(label)?;

    Local
        .from_local_datetime(&time)
        .earliest()
        .map(|time| time.timestamp())
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;
    use crate::history;
    use crate::test_util::TempDir;

    const START: i64 = 1_700_000_000;

    fn backups() -> Backups {
        Backups::init(Connection::open_in_memory().unwrap()).unwrap()
    }

    fn entries(content: &str) -> Vec<Entry> {
        history::parse(content.as_bytes())
    }

    fn commands(entries: &[Entry]) -> Vec<(Option<i64>, &str)> {
        entries
            .iter()
            .map(|entry| (entry.time, entry.original.as_str()))
            .collect()
    }

    fn save(backups: &mut Backups, content: &str, started_at: i64) -> Saved {
        backups.save(&entries(content), "mac", started_at).unwrap()
    }

    #[test]
    fn saves_the_entries_of_a_backup() {
        let mut backups = backups();
        let saved = save(&mut backups, "ls\n#1\npwd\n", START);

        assert_eq!((saved.entries, saved.added), (2, 2));
        assert_eq!(saved.label, label(START));
        assert_eq!(
            commands(&backups.entries(Some(START)).unwrap()),
            [(None, "ls"), (Some(1), "pwd")]
        );
    }

    #[test]
    fn keeps_multi_line_commands_as_they_are() {
        let mut backups = backups();
        save(&mut backups, "#1\nfor x\ndo\techo\n", START);

        assert_eq!(
            commands(&backups.entries(None).unwrap()),
            [(Some(1), "for x\ndo\techo")]
        );
    }

    #[test]
    fn stores_an_entry_saved_again_once() {
        let mut backups = backups();
        save(&mut backups, "#1\nls\n#2\npwd\n", START);
        let saved = save(&mut backups, "#1\nls\n#2\npwd\n#3\nls\n", START + 1);

        assert_eq!((saved.entries, saved.added), (3, 1));
        assert_eq!(backups.entries(None).unwrap().len(), 3);
    }

    #[test]
    fn counts_the_same_command_at_the_same_time_once() {
        let mut backups = backups();
        let saved = save(&mut backups, "ls\nls\n#1\npwd\n#1\npwd\n", START);

        assert_eq!((saved.entries, saved.added), (2, 2));
    }

    #[test]
    fn keeps_each_backup_as_it_was() {
        let mut backups = backups();
        save(&mut backups, "#1\nsecret\n#2\nls\n", START);
        save(&mut backups, "#2\nls\n#3\npwd\n", START + 1);

        assert_eq!(
            commands(&backups.entries(Some(START)).unwrap()),
            [(Some(1), "secret"), (Some(2), "ls")]
        );
        assert_eq!(
            commands(&backups.entries(Some(START + 1)).unwrap()),
            [(Some(2), "ls"), (Some(3), "pwd")]
        );
        assert_eq!(
            commands(&backups.entries(None).unwrap()),
            [(Some(1), "secret"), (Some(2), "ls"), (Some(3), "pwd")]
        );
    }

    #[test]
    fn lists_untimed_entries_first_in_the_order_they_were_saved() {
        let mut backups = backups();
        save(&mut backups, "b\n#5\nc\n", START);
        save(&mut backups, "a\nb\n", START + 1);

        assert_eq!(
            commands(&backups.entries(None).unwrap()),
            [(None, "b"), (None, "a"), (Some(5), "c")]
        );
    }

    #[test]
    fn refuses_a_second_backup_at_the_same_second() {
        let mut backups = backups();
        save(&mut backups, "ls\n", START);

        assert_eq!(
            backups.save(&entries("pwd\n"), "mac", START),
            Err(format!("backup {} already exists, try again", label(START)))
        );
        assert_eq!(commands(&backups.entries(None).unwrap()), [(None, "ls")]);
    }

    #[test]
    fn lists_the_backups_oldest_first() {
        let mut backups = backups();
        save(&mut backups, "ls\npwd\n", START + 60);
        save(&mut backups, "", START);

        assert_eq!(
            backups.list().unwrap(),
            [
                Listed {
                    label: label(START),
                    host: "mac".to_string(),
                    entries: 0,
                },
                Listed {
                    label: label(START + 60),
                    host: "mac".to_string(),
                    entries: 2,
                },
            ]
        );
    }

    #[test]
    fn deletes_a_backup_and_the_entries_only_it_had() {
        let mut backups = backups();
        save(&mut backups, "#1\nsecret\n#2\nls\n", START);
        save(&mut backups, "#2\nls\n", START + 1);

        backups.delete(START).unwrap();

        assert_eq!(backups.list().unwrap().len(), 1);
        assert_eq!(commands(&backups.entries(None).unwrap()), [(Some(2), "ls")]);
        assert!(backups.entries(Some(START)).unwrap().is_empty());
    }

    #[test]
    fn finds_a_backup_by_its_label() {
        let mut backups = backups();
        save(&mut backups, "ls\n", START);

        assert_eq!(backups.find(&label(START)), Ok(Some(START)));
        assert_eq!(backups.find(&label(START + 1)), Ok(None));
    }

    #[test]
    fn labels_backups_with_their_local_time() {
        let time = DateTime::from_timestamp(START, 0)
            .unwrap()
            .with_timezone(&Local);
        let label = label(START);

        assert_eq!(label, time.format("%Y%m%d-%H%M%S").to_string());
        assert_eq!(started_at(&label), Some(START));
    }

    #[test]
    fn reads_only_dates_and_times_as_labels() {
        assert!(parse_label("20261030-142501").is_some());

        for label in [
            "",
            "20261030",
            "20261030-1425",
            "20261030142501",
            "20261030 142501",
            "2026103a-142501",
            "20261330-142501",
            "20261030-246001",
            "+2026103-142501",
            "20261030-142501-mac",
        ] {
            assert_eq!(parse_label(label), None, "{label}");
        }
    }

    #[test]
    fn refuses_a_database_from_a_newer_istoria() {
        let connection = Connection::open_in_memory().unwrap();
        connection.pragma_update(None, "user_version", 2).unwrap();

        assert!(Backups::init(connection).is_err());
    }

    #[test]
    fn opens_a_database_it_created_only_the_owner_can_read() {
        let dir = TempDir::new("backups");
        let path = dir.path.join("istoria").join("history.db");
        let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;

        Backups::open(&path)
            .unwrap()
            .save(&entries("ls\n"), "mac", START)
            .unwrap();
        let backups = Backups::open_existing(&path).unwrap().unwrap();

        assert_eq!(commands(&backups.entries(None).unwrap()), [(None, "ls")]);
        assert_eq!(mode(path.parent().unwrap()), 0o700);
        assert_eq!(mode(&path), 0o600);
    }

    #[test]
    fn opens_no_database_that_is_missing() {
        let path = env::temp_dir()
            .join("istoria-test-missing")
            .join("history.db");

        assert!(Backups::open_existing(&path).unwrap().is_none());
        assert!(!path.exists());
    }
}
