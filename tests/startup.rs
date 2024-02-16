use pgtemp::PgTempDB;

#[test]
/// We can bring up a temp db and its data directory is gone after dropping it.
fn test_tempdb_bringup_shutdown() {
    let db = PgTempDB::new();
    let data_dir = db.data_dir().clone();
    let conf_file = data_dir.join("postgresql.conf");

    assert!(conf_file.exists());

    drop(db);

    assert!(!conf_file.exists());
}

#[tokio::test]
/// Async version of tempdb_bringup_shutdown
async fn test_tempdb_bringup_shutdown_async() {
    let db = PgTempDB::async_new().await;
    let data_dir = db.data_dir().clone();
    let conf_file = data_dir.join("postgresql.conf");

    assert!(conf_file.exists());

    drop(db);

    assert!(!conf_file.exists());
}

#[test]
/// We can bring up a temp db and its data directory is saved when enabling the persist flag.
fn test_tempdb_bringup_shutdown_persist() {
    let temp = tempfile::tempdir().unwrap(); // just so we don't have to manually clean up at the
                                             // end of the test
    let db = PgTempDB::builder()
        .persist_data(true)
        .with_data_dir_prefix(temp.path())
        .start();
    let data_dir = db.data_dir().clone();
    let conf_file = data_dir.join("postgresql.conf");

    assert!(conf_file.exists());

    drop(db);

    assert!(conf_file.exists());
}
