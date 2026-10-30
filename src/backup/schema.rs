//! The tables of the backups database and their version

use rusqlite::{Connection, Transaction, TransactionBehavior};

/// The `user_version` schema.sql sets
const SCHEMA_VERSION: i64 = 1;

const SCHEMA: &str = include_str!("schema.sql");

/// Creates the tables in a new database. Refuses a database from a newer istoria.
pub(super) fn prepare(connection: &Connection) -> Result<(), String> {
    match setup(connection).map_err(|error| error.to_string())? {
        SCHEMA_VERSION => Ok(()),
        version => Err(format!("schema version {version}, from a newer istoria")),
    }
}

/// Returns the schema version
fn setup(connection: &Connection) -> rusqlite::Result<i64> {
    match schema_version(connection)? {
        0 => {
            create_schema(connection)?;
            schema_version(connection)
        }
        version => Ok(version),
    }
}

fn schema_version(connection: &Connection) -> rusqlite::Result<i64> {
    connection.pragma_query_value(None, "user_version", |row| row.get(0))
}

/// Unless another istoria created it meanwhile
fn create_schema(connection: &Connection) -> rusqlite::Result<()> {
    let transaction = Transaction::new_unchecked(connection, TransactionBehavior::Immediate)?;

    if schema_version(&transaction)? == 0 {
        transaction.execute_batch(SCHEMA)?;
    }

    transaction.commit()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_the_schema_at_its_version() {
        let connection = Connection::open_in_memory().unwrap();

        assert_eq!(prepare(&connection), Ok(()));
        assert_eq!(schema_version(&connection).unwrap(), SCHEMA_VERSION);
    }

    #[test]
    fn refuses_a_database_from_a_newer_istoria() {
        let connection = Connection::open_in_memory().unwrap();
        connection.pragma_update(None, "user_version", 2).unwrap();

        assert_eq!(
            prepare(&connection),
            Err("schema version 2, from a newer istoria".to_string())
        );
    }
}
