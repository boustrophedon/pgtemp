//! Test basic functionality

use pgtemp::{PgTempDB, PgTempDBBuilder};
use sqlx::postgres::PgConnection;
use sqlx::prelude::*;

#[tokio::test]
/// check database name is correct
async fn check_database_name() {
    // test default name
    let db = PgTempDB::new();
    assert_eq!(db.db_name(), "postgres");

    let mut conn = PgConnection::connect(&db.connection_uri())
        .await
        .expect("failed to connect to db");

    let row = sqlx::query("SELECT current_database()")
        .fetch_one(&mut conn)
        .await
        .expect("failed to execute current db query");

    let name: String = row.get(0);
    assert_eq!(name, "postgres");

    drop(conn);
    drop(db);

    // test with custom name
    let db = PgTempDB::builder().with_dbname("my_cool_temp_db").start();
    assert_eq!(db.db_name(), "my_cool_temp_db");

    let mut conn = PgConnection::connect(&db.connection_uri())
        .await
        .expect("failed to connect to db");

    let row = sqlx::query("SELECT current_database()")
        .fetch_one(&mut conn)
        .await
        .expect("failed to execute current db query");

    let name: String = row.get(0);
    assert_eq!(name, "my_cool_temp_db");
}

#[tokio::test]
/// check all setters work
async fn builder_setters() {
    // test default name
    let mut db = PgTempDB::builder()
        .with_username("testuser")
        .with_password("potato")
        .with_port(9954)
        .with_dbname("testdb1")
        .with_config_param("max_connections", "777")
        .with_initdb_arg("encoding", "UTF8")
        .with_initdb_arg("locale", "C");
    assert_eq!(db.get_user(), "testuser");
    assert_eq!(db.get_password(), "potato");
    assert_eq!(db.get_port_or_set_random(), 9954);
    assert_eq!(db.get_dbname(), "testdb1");

    let mut db2 =
        PgTempDBBuilder::from_connection_uri("postgresql://testuser:potato@localhost:9954/testdb1");
    assert_eq!(db.get_user(), db2.get_user());
    assert_eq!(db.get_password(), db2.get_password());
    assert_eq!(db.get_port_or_set_random(), db2.get_port_or_set_random());
    assert_eq!(db.get_dbname(), db2.get_dbname());

    let db = db.start();
    // test the debug and libpq conn strings formatters don't panic
    println!("{:?}", db);
    println!("{}", db.connection_string());
    let mut conn = PgConnection::connect(&db.connection_uri())
        .await
        .expect("failed to connect to db");

    let row = sqlx::query("SELECT current_database()")
        .fetch_one(&mut conn)
        .await
        .expect("failed to execute current db query");

    let name: String = row.get(0);
    assert_eq!(name, "testdb1");

    // check config param setting as well
    let row = sqlx::query("SELECT setting from pg_settings WHERE name = 'max_connections'")
        .fetch_one(&mut conn)
        .await
        .expect("failed to execute current db query");

    let name: &str = row.get(0);
    assert_eq!(name, "777");

    // check initdb param setting as well
    let row = sqlx::query("SHOW server_encoding")
        .fetch_one(&mut conn)
        .await
        .expect("failed to execute server_encoding query");

    let encoding: &str = row.get(0);
    assert_eq!(encoding, "UTF8");

    // check the locale settings
    let row = sqlx::query("SELECT datcollate from pg_database")
        .fetch_one(&mut conn)
        .await
        .expect("failed to execute LC_COLLATE query");

    let lc_collate: &str = row.get(0);
    assert_eq!(lc_collate, "C");

    let row = sqlx::query("SELECT datctype from pg_database")
        .fetch_one(&mut conn)
        .await
        .expect("failed to execute LC_CTYPE query");

    let lc_ctype: &str = row.get(0);
    assert_eq!(lc_ctype, "C");
}

#[tokio::test]
/// create a table and insert into it
async fn create_table_and_insert() {
    let db = PgTempDB::new();

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

    let name = "example name";
    sqlx::query("INSERT INTO person (name) VALUES ($1)")
        .bind(name)
        .execute(&mut conn)
        .await
        .expect("failed to insert name into values");

    let rows = sqlx::query("SELECT id, name FROM person")
        .fetch_all(&mut conn)
        .await
        .expect("failed to select names from person");
    assert_eq!(rows.len(), 1);

    let row = &rows[0];
    let id: i32 = row.get(0);
    let name: &str = row.get(1);

    assert_eq!(id, 1);
    assert_eq!(name, "example name");
}
