use pgtemp::PgTempDB;

#[test]
/// We can bring up a temp db and its data directory is gone after dropping it.
fn test_tempdb_bringup_shutdown() {
    let db = PgTempDB::new();
    let data_dir = db.data_dir().clone();
    let conf = data_dir.join("postgresql.conf");

    // Read the conf file and check it isn't empty and the port is correct
    let res = std::fs::read_to_string(&conf);
    assert!(res.is_ok());
    let text = res.unwrap();
    assert!(text.len() > 0);
    assert!(text.contains(&format!("port = {}", db.db_port())));
    drop(db);

    // we only need to sleep here in order to let the cleanup thread run - in a regular test you
    // wouldn't need to sleep because the test would just end.
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(!conf.exists());
}

#[test]
/// Async version of tempdb_bringup_shutdown
fn test_tempdb_bringup_shutdown_async() {
   let conf = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let db = PgTempDB::async_new().await;
            let data_dir = db.data_dir().clone();
            let conf = data_dir.join("postgresql.conf");

            // Read the conf file and check it isn't empty and the port is correct
            let res = tokio::fs::read_to_string(&conf).await;
            assert!(res.is_ok());
            let text = res.unwrap();
            assert!(text.len() > 0);
            assert!(text.contains(&format!("port = {}", db.db_port())));
            drop(db);

            conf
        });

        // we only need to sleep here in order to let the cleanup thread run - in a regular
        // test you wouldn't need to sleep because the test would just end.
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert!(!conf.exists());
}
