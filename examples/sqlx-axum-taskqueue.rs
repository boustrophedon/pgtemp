/// # Sqlx task queue example
/// This provides an example of using triggers, sql functions, and listen/notify with pgtemp to
/// demonstrate that pgtemp works with more advanced postgres features.
///
/// A connection pool is created, and sqlx migrations (in the `examples/sqlx-migrations` directory)
/// are run on startup.
///
/// The migrations contain a tasks table along with a trigger and function that sends a
/// notification to the postgresql "insert_tasks" NOTIFY channel when a row is inserted to the
/// tasks table.
///
/// There is a thread (`run_listener`) that is spawned separately which listens to the
/// instert_tasks channel and selects a task to "execute" (here just printing out the task text).
///
/// There is an axum server with a `create_task` and `list_completed_tasks` endpoint.
///
/// We create two tasks, wait a second, and then check that when we list all completed tasks both
/// tasks have been processed by the listener and marked complete.
///
/// # How this example was set up
///
/// Install the sqlx CLI
/// `cargo install sqlx-cli --no-default-features --features native-tls,postgres`
///
/// Set up the migration
///
/// `sqlx migrate add -r --source examples/sqlx-migrations create_tasks_table`
///
/// The sqlx CLI does not require a running database in this case.
use axum::{
    extract::State,
    routing::{get, post},
    Router,
};

use sqlx::prelude::*;

type PgPool = sqlx::postgres::PgPool;

async fn connection_pool(conn_uri: &str) -> PgPool {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect(conn_uri)
        .await
        .expect("failed to create connection pool");

    sqlx::migrate!("examples/sqlx-migrations")
        .run(&pool)
        .await
        .expect("failed to run migrations");
    pool
}

async fn list_completed_tasks(pool: State<PgPool>) -> String {
    let rows = sqlx::query("SELECT task FROM tasks WHERE completed = true")
        .fetch_all(&*pool)
        .await
        .expect("failed to execute select query");
    let mut output = String::new();
    for row in rows {
        let task: &str = row.get(0);
        output = output + task + "\n";
    }
    output
}

async fn create_task(pool: State<PgPool>, body: String) -> &'static str {
    sqlx::query("INSERT INTO tasks (task) VALUES ($1)")
        .bind(body)
        .execute(&*pool)
        .await
        .expect("failed to execute insert query");

    // If you don't want to notify in the trigger and instead want to notify via sql
    // sqlx::query("NOTIFY insert_tasks")
    //     .execute(&*pool)
    //     .await
    //     .expect("failed to execute task notification");
    "ok"
}

fn axum_router(pool: PgPool) -> Router {
    Router::new()
        .route("/list_completed_tasks", get(list_completed_tasks))
        .route("/create_task", post(create_task))
        .with_state(pool)
}

async fn run_listener(conn_uri: &str) {
    // NOTE: if you wanted this to be a "real" queue, you'd have to do a few more things like:
    // - handle existing events on startup
    // - handle missed notifications from disconnects (e.g. network blips)
    // - handle failed tasks (because you won't get another NOTIFY via the trigger as it's set up
    // currently)
    // - add more listeners in parallel

    let pool = connection_pool(&conn_uri).await;
    // set up listener
    let mut pglistener = sqlx::postgres::PgListener::connect_with(&pool)
        .await
        .expect("failed to create listener");

    pglistener
        .listen("insert_tasks")
        .await
        .expect("failed to start listening to insert_task events");

    println!("listener is ready");
    loop {
        let notif = pglistener.recv().await.expect("listener recv failed.");
        let row_id = notif.payload();
        println!("new task, row id {row_id}");

        // Here we could use the row_id from the notification instead of select completed = false
        // but it's better as an example because you could use the same query in a loop in another
        // thread to look for tasks that got missed, or to amortize processing all of them at once
        // on startup.

        // start transaction
        let mut tx = pool.begin().await.expect("failed to start transaction");

        // get a single task from the queue, locking the row with `FOR UPDATE` and skipping already
        // locked rows
        let task_row = sqlx::query(
            "SELECT id, task FROM tasks WHERE completed = false FOR UPDATE SKIP LOCKED LIMIT 1",
        )
        .fetch_one(&mut *tx)
        .await
        .expect("failed to select open task");
        let id: i32 = task_row.get(0);
        let task: &str = task_row.get(1);
        println!("executing task `{task}`");
        // Here you could do something like:
        // If the task failed, increment an "execution attempts" row and commit but don't set
        // `completed = true`

        sqlx::query("UPDATE tasks SET completed = true WHERE id = ($1)")
            .bind(id)
            .execute(&mut *tx)
            .await
            .expect("failed to execute update on task");

        tx.commit()
            .await
            .expect("failed to commit task execution transaction");

        // And we can even notify that we've executed tasks
        sqlx::query("NOTIFY task_executed")
            .execute(&pool)
            .await
            .expect("failed to notify task executed");
    }
}

#[tokio::test]
async fn test_sqlx_queue_example() {
    run_sqlx_queue_example().await;
}

#[tokio::main]
async fn main() {
    run_sqlx_queue_example().await;
}

async fn run_sqlx_queue_example() {
    // start db
    let db = pgtemp::PgTempDB::new();
    let conn_uri = db.connection_uri().clone();

    let pool = connection_pool(&conn_uri).await;

    // create queue execution thread runner
    // Note that here we're re-using the same connection pool, but we could just as easily pass the
    // conn_uri from above and make a new pool and it would connect to the same database. I don't
    // think there's anything wrong with using the same pool in two different tokio runtimes.
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("failed to start runtime");
        rt.block_on(async { run_listener(&conn_uri).await });
    });

    // give the listener some time to start
    std::thread::sleep(std::time::Duration::from_millis(200));

    // create a pglistener to listen for task execution events
    let mut pglistener = sqlx::postgres::PgListener::connect_with(&pool)
        .await
        .expect("failed to create listener");

    pglistener
        .listen("task_executed")
        .await
        .expect("failed to start listening to task_executed events");

    // create axum router and spawn listener
    let router = axum_router(pool.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to start listener");
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("failed to run axum server");
    });

    // add two tasks
    let base_url = format!("http://{addr}");
    let client = reqwest::Client::new();

    let resp = client
        .post(base_url.clone() + "/create_task")
        .body("hello")
        .send()
        .await
        .expect("failed to create task 1");
    assert!(resp.status().is_success());

    let resp = client
        .post(base_url.clone() + "/create_task")
        .body("task 2")
        .send()
        .await
        .expect("failed to create task 2");
    assert!(resp.status().is_success());

    // wait for two execution events
    let _notif1 = pglistener.recv().await.expect("listener recv failed.");
    let _notif2 = pglistener.recv().await.expect("listener recv failed.");

    // query tasks and check both have been executed
    let resp = client
        .get(base_url + "/list_completed_tasks")
        .send()
        .await
        .expect("failed to list tasks");
    assert!(resp.status().is_success());

    let body = resp.text().await.expect("failed to parse body");
    assert!(body.contains("hello"));
    assert!(body.contains("task 2"));
}
