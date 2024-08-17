use sqlx::postgres::PgConnection;
use sqlx::prelude::*;

type PgConn = PgConnection;

#[tokio::test]
/// Test we can spawn the daemon and connect to it
async fn spawn_daemon_get_database_name() {
    tokio::spawn(pgtemp::PgTempDaemon::async_new(5433).await.start());

    let mut conn = PgConnection::connect("postgres://postgres:password@localhost:5433")
        .await
        .expect("failed to connect to db");

    let row = sqlx::query("SELECT current_database()")
        .fetch_one(&mut conn)
        .await
        .expect("failed to execute current db query");

    let name: String = row.get(0);
    assert_eq!(name, "postgres");
}

#[tokio::test]
/// create a table and insert into it, daemon version of basic_operations.rs test
async fn spawn_daemon_create_table_and_insert() {
    tokio::spawn(pgtemp::PgTempDaemon::async_new(5434).await.start());

    let mut conn = PgConnection::connect("postgres://postgres:password@localhost:5434")
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

#[tokio::test]
/// create two dbs and insert into them, checking that they are independent
async fn daemon_two_dbs_are_independent() {
    tokio::spawn(pgtemp::PgTempDaemon::async_new(2434).await.start());

    // same server but you get different databases
    let mut conn1 = PgConnection::connect("postgres://postgres:password@localhost:2434")
        .await
        .expect("failed to connect to db");
    let mut conn2 = PgConnection::connect("postgres://postgres:password@localhost:2434")
        .await
        .expect("failed to connect to db");

    create_table(&mut conn1).await;
    create_table(&mut conn2).await;

    insert_name(&mut conn1, "test1").await;
    insert_name(&mut conn2, "test2").await;

    check_data(&mut conn1, "test1").await;
    check_data(&mut conn2, "test2").await;
}

#[cfg(feature = "cli")]
#[tokio::test]
/// Test single mode (and cli args)
async fn daemon_single_mode() {
    let uri = "postgresql://postgres:password@localhost:1434";
    let temp_prefix = tempfile::tempdir().unwrap();
    let args = pgtemp::PgTempDaemonArgs {
        single: true,
        data_dir_prefix: Some(temp_prefix.path().into()),
        load_from: None,
        server_params: vec![("geqo".into(), "off".into()), ("jit".into(), "off".into())],
        connection_uri: uri.to_string(),
    };

    let daemon = pgtemp::PgTempDaemon::from_args(args).await;
    tokio::spawn(daemon.start());

    // both connect to the same server
    let mut conn1 = PgConnection::connect(uri)
        .await
        .expect("failed to connect to db");
    let mut conn2 = PgConnection::connect(uri)
        .await
        .expect("failed to connect to db");

    // check the config params were set
    check_config(&mut conn1).await;

    // create table on conn 2
    create_table(&mut conn2).await;

    // insert data on conn 1
    insert_name(&mut conn1, "test").await;

    // check data on both
    check_data(&mut conn1, "test").await;
    check_data(&mut conn2, "test").await;
}

#[cfg(feature = "cli")]
async fn check_config(conn: &mut PgConn) {
    // jit, ssl, and geqo are the shorted postgres config names but we can't turn on ssl
    let rows = sqlx::query("SELECT name, setting from pg_settings WHERE name = 'jit' OR name = 'geqo' ORDER BY name ASC")
        .fetch_all(conn)
        .await
        .expect("failed to get config settings");
    assert_eq!(rows.len(), 2);

    let row = &rows[1];
    let geqo: &str = row.get(1);
    assert_eq!(geqo, "off");

    let row = &rows[0];
    let jit: &str = row.get(1);
    assert_eq!(jit, "off");
}

async fn create_table(conn: &mut PgConn) {
    sqlx::query(
        "
        CREATE TABLE person (
            id      SERIAL PRIMARY KEY,
            name    TEXT NOT NULL
        )
    ",
    )
    .execute(conn)
    .await
    .expect("failed to create table");
}

async fn insert_name(conn: &mut PgConn, name: &str) {
    sqlx::query("INSERT INTO person (name) VALUES ($1)")
        .bind(name)
        .execute(conn)
        .await
        .expect("failed to insert name into values");
}

async fn check_data(conn: &mut PgConn, name: &str) {
    let rows = sqlx::query("SELECT id, name FROM person")
        .fetch_all(conn)
        .await
        .expect("failed to select names from person");
    assert_eq!(rows.len(), 1);

    let row = &rows[0];
    let id: i32 = row.get(0);
    let fetched_name: &str = row.get(1);

    assert_eq!(id, 1);
    assert_eq!(fetched_name, name);
}
