//! Transitional connection owner. Both repositories use the legacy mutex as one
//! application gate; each transaction runs entirely on one SQLite connection.
//! Remove that compatibility gate when all repositories use Diesel.
use diesel::{Connection, SqliteConnection, connection::SimpleConnection};
use std::fmt;

#[derive(Debug)]
pub enum DbError {
    Query(diesel::result::Error),
    Message(String),
}
impl From<diesel::result::Error> for DbError {
    fn from(e: diesel::result::Error) -> Self {
        Self::Query(e)
    }
}
impl From<String> for DbError {
    fn from(e: String) -> Self {
        Self::Message(e)
    }
}
impl From<&str> for DbError {
    fn from(e: &str) -> Self {
        Self::Message(e.into())
    }
}
impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Query(e) => write!(f, "本地数据操作失败：{e}"),
            Self::Message(s) => f.write_str(s),
        }
    }
}
pub type DbResult<T> = Result<T, DbError>;

pub fn connect(url: &str) -> Result<SqliteConnection, String> {
    let mut db = SqliteConnection::establish(url).map_err(|e| format!("无法打开 SQLite：{e}"))?;
    // Connection-local SQLite controls cannot be expressed as row CRUD.
    db.batch_execute("PRAGMA foreign_keys=ON; PRAGMA busy_timeout=3000; PRAGMA synchronous=FULL;")
        .map_err(|e| e.to_string())?;
    Ok(db)
}

impl super::Storage {
    pub(super) fn typed<T>(
        &self,
        operation: impl FnOnce(&mut SqliteConnection) -> DbResult<T>,
    ) -> Result<T, String> {
        let _gate = self.db.lock().unwrap();
        let mut db = self.typed_db.lock().unwrap();
        operation(&mut db).map_err(|e| e.to_string())
    }
    pub(super) fn typed_transaction<T>(
        &self,
        operation: impl FnOnce(&mut SqliteConnection) -> DbResult<T>,
    ) -> Result<T, String> {
        self.typed(|db| db.transaction(operation))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn diesel_schema_matches_every_migrated_sqlite_column_and_primary_key() {
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        super::super::migrate(&mut db).unwrap();
        let schema = include_str!("schema.rs");
        let mut declared_tables = Vec::new();
        for block in schema.split("diesel::table! {").skip(1) {
            let mut lines = block.trim_start().lines();
            let declaration = lines.next().unwrap();
            let table = declaration.split_whitespace().next().unwrap();
            declared_tables.push(table.to_owned());
            let primary: Vec<_> = declaration
                .split_once('(')
                .unwrap()
                .1
                .split_once(')')
                .unwrap()
                .0
                .split(',')
                .map(str::trim)
                .collect();
            let declared: Vec<_> = lines
                .take_while(|line| line.trim() != "}")
                .map(|line| {
                    let (name, ty) = line
                        .trim()
                        .trim_end_matches(',')
                        .split_once(" -> ")
                        .unwrap();
                    let nullable = ty.starts_with("Nullable<");
                    let base = ty.trim_start_matches("Nullable<").trim_end_matches('>');
                    (
                        name.to_owned(),
                        match base {
                            "Text" => "TEXT",
                            "BigInt" => "INTEGER",
                            other => panic!("unhandled schema type {other}"),
                        }
                        .to_owned(),
                        nullable,
                    )
                })
                .collect();
            // Schema introspection is SQLite metadata, not application CRUD.
            let actual: Vec<(String, String, bool, i64)> = db
                .prepare(&format!("PRAGMA table_info({table})"))
                .unwrap()
                .query_map([], |r| {
                    let not_null: bool = r.get(3)?;
                    let pk: i64 = r.get(5)?;
                    Ok((r.get(1)?, r.get(2)?, !not_null && pk == 0, pk))
                })
                .unwrap()
                .collect::<rusqlite::Result<_>>()
                .unwrap();
            assert_eq!(
                declared,
                actual
                    .iter()
                    .map(|(name, ty, nullable, _)| (name.clone(), ty.clone(), *nullable))
                    .collect::<Vec<_>>(),
                "schema drift in {table}"
            );
            let mut keys: Vec<_> = actual.iter().filter(|row| row.3 > 0).collect();
            keys.sort_by_key(|row| row.3);
            assert_eq!(
                primary,
                keys.iter().map(|row| row.0.as_str()).collect::<Vec<_>>(),
                "primary key drift in {table}"
            );
        }
        let mut tables: Vec<String> = db
            .prepare(
                "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
            )
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        tables.sort();
        declared_tables.sort();
        assert_eq!(declared_tables, tables);
    }
}
