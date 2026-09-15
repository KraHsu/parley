//! One Diesel SQLite connection and one access lock shared by all repositories.
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
    db.batch_execute("PRAGMA busy_timeout=3000;")
        .map_err(|e| e.to_string())?;
    Ok(db)
}

impl super::Storage {
    pub(super) fn repository<T>(
        &self,
        operation: impl FnOnce(&mut SqliteConnection) -> Result<T, String>,
    ) -> Result<T, String> {
        self.typed(|db| operation(db).map_err(Into::into))
    }
    pub(super) fn repository_transaction<T>(
        &self,
        operation: impl FnOnce(&mut SqliteConnection) -> Result<T, String>,
    ) -> Result<T, String> {
        self.typed_transaction(|db| operation(db).map_err(Into::into))
    }
    pub(super) fn typed<T>(
        &self,
        operation: impl FnOnce(&mut SqliteConnection) -> DbResult<T>,
    ) -> Result<T, String> {
        let mut db = self.db.lock().unwrap();
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
        use diesel::RunQueryDsl;
        let mut db = super::connect(":memory:").unwrap();
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
            #[derive(diesel::QueryableByName)]
            struct Column {
                #[diesel(sql_type=diesel::sql_types::Text)]
                name: String,
                #[diesel(sql_type=diesel::sql_types::Text,column_name="type")]
                ty: String,
                #[diesel(sql_type=diesel::sql_types::BigInt)]
                notnull: i64,
                #[diesel(sql_type=diesel::sql_types::BigInt)]
                pk: i64,
            }
            let mut actual: Vec<(String, String, bool, i64)> =
                diesel::sql_query(format!("PRAGMA table_info({table})"))
                    .load::<Column>(&mut db)
                    .unwrap()
                    .into_iter()
                    .map(|r| (r.name, r.ty, r.notnull == 0 && r.pk == 0, r.pk))
                    .collect();
            if declared.first().is_some_and(|row| row.0 == "rowid") {
                // These existing rowid tables use insertion order for sources and undo.
                // SQLite's implicit rowid is not listed by table_info.
                assert_eq!(
                    super::super::test_support::integer(
                        &mut db,
                        &format!("SELECT count(rowid) AS value FROM {table}")
                    ),
                    super::super::test_support::integer(
                        &mut db,
                        &format!("SELECT count(*) AS value FROM {table}")
                    )
                );
                actual.insert(0, ("rowid".into(), "INTEGER".into(), false, 0));
            }
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
        #[derive(diesel::QueryableByName)]
        struct Table {
            #[diesel(sql_type=diesel::sql_types::Text)]
            name: String,
        }
        let mut tables: Vec<String> = diesel::sql_query(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
        )
        .load::<Table>(&mut db)
        .unwrap()
        .into_iter()
        .map(|r| r.name)
        .collect();
        tables.sort();
        declared_tables.sort();
        assert_eq!(declared_tables, tables);
    }
}
