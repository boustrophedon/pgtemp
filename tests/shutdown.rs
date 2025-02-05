#[tokio::test]
async fn pgtemp_shutdown_test() {
    use pgtemp::PgTempDB;
    use sqlx::postgres::PgPoolOptions;

    let temp_db = PgTempDB::async_new().await;
    println!("got  temp DB");
    let address = format!(
        "postgresql://{}:{}@localhost:{}",
        temp_db.db_user(),
        temp_db.db_pass(),
        temp_db.db_port()
    );

    let pool = PgPoolOptions::new()
        .connect(&address)
        .await
        .expect("Can't connect to postgres");

    let conn = pool.acquire().await.unwrap();
    drop(conn);

    println!("starting teardown");
    pool.close().await;
    println!("pool2: size: {} idle: {}", pool.size(), pool.num_idle());
    drop(pool);
    println!("closing temp DB");
    drop(temp_db);
    println!("teardown complete");
}
