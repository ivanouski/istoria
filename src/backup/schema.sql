-- A backup is its start, in seconds, and the entries it saw. An entry without a timestamp has the
-- time 0, as NULLs would never be equal in the unique key.

PRAGMA user_version = 1;

CREATE TABLE backups (
  started_at INTEGER PRIMARY KEY,
  host       TEXT NOT NULL
);

CREATE TABLE entries (
  id      INTEGER PRIMARY KEY,
  time    INTEGER NOT NULL,
  command TEXT NOT NULL,
  UNIQUE (time, command)
);

CREATE TABLE entry_backups (
  started_at INTEGER NOT NULL REFERENCES backups ON DELETE CASCADE,
  entry_id   INTEGER NOT NULL REFERENCES entries ON DELETE CASCADE,
  PRIMARY KEY (started_at, entry_id)
) WITHOUT ROWID;

CREATE INDEX entry_backups_entry_id ON entry_backups (entry_id);
