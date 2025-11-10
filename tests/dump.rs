//! Tests for the dump and restore functionality

use std::io::Write;

use pgtemp::PgTempDB;
use sqlx::postgres::PgConnection;
use sqlx::prelude::*;

// TODO: test dump and restore with migrations

#[tokio::test]
/// create a table and insert into it, then dump it and restore it to a new database
async fn dump_and_restore() {
    let temp = tempfile::tempdir().unwrap();
    let dump_path = temp.path().join("dump.sql");

    let db = PgTempDB::builder()
        .dump_database(&dump_path)
        .start_async()
        .await;

    let mut conn = PgConnection::connect(&db.connection_uri())
        .await
        .expect("failed to connect to db");

    sqlx::query(
        "
        CREATE TABLE person (
            id      SERIAL PRIMARY KEY,
            name    TEXT NOT NULL
        )
    ",
    )
    .execute(&mut conn)
    .await
    .expect("failed to create table");

    for i in 0..10 {
        let name = format!("example name {}", i);
        sqlx::query("INSERT INTO person (name) VALUES ($1)")
            .bind(name)
            .execute(&mut conn)
            .await
            .expect("failed to insert name into values");
    }

    drop(conn);
    drop(db); // database is dumped here

    // start a new database and load the data
    let db = PgTempDB::builder()
        .load_database(&dump_path)
        .start_async()
        .await;

    let mut conn = PgConnection::connect(&db.connection_uri())
        .await
        .expect("failed to connect to db");
    // check the data is there
    let rows = sqlx::query("SELECT id, name FROM person ORDER BY name ASC")
        .fetch_all(&mut conn)
        .await
        .expect("failed to select names from person");
    assert_eq!(rows.len(), 10);

    let row = &rows[0];
    let id: i32 = row.get(0);
    let name: &str = row.get(1);

    assert_eq!(id, 1);
    assert_eq!(name, "example name 0");

    let row = &rows[9];
    let id: i32 = row.get(0);
    let name: &str = row.get(1);

    assert_eq!(id, 10);
    assert_eq!(name, "example name 9");
}

#[tokio::test]
/// make sure that we correctly error on bad database dumps.
#[should_panic(expected = "syntax error at or near \\\"INVALID\\")]
async fn panic_on_load_error() {
    let temp = tempfile::tempdir().unwrap();
    let db_dump_path = temp.path().join("dump.sql");

    // Create some bad database dumps
    let mut f = std::fs::File::create(&db_dump_path).unwrap();
    f.write_all(b"INVALID SQL").unwrap();
    f.flush().expect("Failed to flush file");

    // Try to load it (it should fail)
    PgTempDB::builder()
        .load_database(&db_dump_path)
        .start_async()
        .await;
}
