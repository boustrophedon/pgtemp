// TODO: these are disabled in ci because ci doesn't have postgres16 (and presumably neither does
// many existing deployments) but really they aren't needed since all the other tests basically
// test these things anyway
#![cfg(feature = "pg16")]
use pgtemp::PgTempDB;

#[test]
/// We can bring up a temp db and its data directory is gone after dropping it.
fn test_tempdb_bringup_shutdown() {
    let db = PgTempDB::new();
    let data_dir = db.data_dir().clone();
    let conf_file = data_dir.join("postgresql.conf");

    // Read the conf file and check it isn't empty and the port is correct
    let res = std::fs::read_to_string(&conf_file);
    assert!(res.is_ok());
    let text = res.unwrap();
    assert!(!text.is_empty());
    assert!(text.contains(&format!("port = {}", db.db_port())));
    drop(db);

    assert!(!conf_file.exists());
}

#[tokio::test]
/// Async version of tempdb_bringup_shutdown
async fn test_tempdb_bringup_shutdown_async() {
    let db = PgTempDB::async_new().await;
    let data_dir = db.data_dir().clone();
    let conf_file = data_dir.join("postgresql.conf");

    // Read the conf file and check it isn't empty and the port is correct
    let res = tokio::fs::read_to_string(&conf_file).await;
    assert!(res.is_ok());
    let text = res.unwrap();
    assert!(!text.is_empty());
    assert!(text.contains(&format!("port = {}", db.db_port())));
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
    let port = db.db_port();
    drop(db);

    // Read the conf file and check it isn't empty and the port is correct
    let res = std::fs::read_to_string(&conf_file);
    assert!(res.is_ok());
    let text = res.unwrap();
    assert!(!text.is_empty());
    assert!(text.contains(&format!("port = {}", port)));

    assert!(conf_file.exists());
}

#[test]
/// Test extra config parameter option
fn test_extra_config_setter() {
    let db = PgTempDB::builder()
        .with_config_param("max_connections", "1")
        .start();

    let data_dir = db.data_dir().clone();
    let conf_file = data_dir.join("postgresql.conf");

    // Read the conf file and check it has the setting
    let res = std::fs::read_to_string(conf_file);
    assert!(res.is_ok());
    let text = res.unwrap();
    assert!(text.contains("max_connections = 1"));
}
