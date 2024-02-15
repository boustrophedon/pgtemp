/// Just some random tests to make coverage happy
#[test]
fn misc() {
    let db = pgtemp::PgTempDB::builder()
        .persist_data(true)
        .with_config_param("shared_buffers", "2GB")
        .with_config_param("work_mem", "1GB")
        .start();
    println!("{:?}", db);
    println!("{}", db.connection_string());
}
