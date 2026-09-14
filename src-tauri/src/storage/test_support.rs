//! Raw SQL fixtures and independent assertions for migration/failure-injection tests.
use diesel::{RunQueryDsl, SqliteConnection};
#[derive(diesel::QueryableByName)]
struct Integer {
    #[diesel(sql_type=diesel::sql_types::BigInt)]
    value: i64,
}
#[derive(diesel::QueryableByName)]
struct Text {
    #[diesel(sql_type=diesel::sql_types::Text)]
    value: String,
}
pub(crate) fn integer(db: &mut SqliteConnection, sql: &str) -> i64 {
    diesel::sql_query(sql)
        .get_result::<Integer>(db)
        .unwrap()
        .value
}
pub(crate) fn text(db: &mut SqliteConnection, sql: &str) -> String {
    diesel::sql_query(sql).get_result::<Text>(db).unwrap().value
}
