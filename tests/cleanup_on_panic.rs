//! Cleanup on panic

/// demonstrates standard rust behavior that typically when a function panics, drop is still
/// called (although this is not guaranteed in all cases)
#[test]
fn drop_after_panic() {
    let db = pgtemp::PgTempDB::new();
    let path = db.data_dir();

    let handle = std::thread::spawn(move || {
        println!("db server running on port {}", db.db_port());
        panic!("oh no");
    });

    // the thread panicked
    let res = handle.join();
    assert!(res.is_err());

    // but the directory was still cleaned up
    assert!(!path.exists());

    // TODO: check postgres process is dead as well somehow - just get pid and check /proc?
}
