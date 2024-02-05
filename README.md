# pgtemp

[![Coverage Status](https://coveralls.io/repos/github/boustrophedon/pgtemp/badge.svg?branch=master)](https://coveralls.io/github/boustrophedon/pgtemp?branch=master) [![CI Status](https://github.com/boustrophedon/pgtemp/actions/workflows/build-test.yaml/badge.svg)](https://github.com/boustrophedon/pgtemp/actions/workflows/build-test.yaml) [![crates.io](https://img.shields.io/crates/v/pgtemp)](https://crates.io/crates/pgtemp) [![docs.rs](https://img.shields.io/docsrs/pgtemp)](https://docs.rs/pgtemp/latest/pgtemp/)

pgtemp is a Rust library and daemon that allows you to easily create temporary PostgreSQL databases (technically clusters) for testing.

The pgtemp Rust library allows you to spawn a PostgreSQL server in a temporary directory and get back the host, port, username, and password.

The pgtemp binary allows you to even more simply make temporary connections: Run pgtemp and then use its connection URI when connecting to the database in your tests. **pgtemp will then spawn a new postgresql process for each connection it receives** and transparently proxy everything over that connection to the temporary database. Note that this means when you make multiple connections in a single test, changes made in one connection will not be visible in the other connections.

pgtemp supports loading (and dumping, in the library) the database to/from [dumpfiles via `pg_dump`](https://www.postgresql.org/docs/current/backup-dump.html).

# Design

pgtemp is a fairly simple program and there are other existing libraries like [testing.postgresql for Python](https://github.com/tk0miya/testing.postgresql) and [pgtest for Go](https://github.com/rubenv/pgtest) that all work the same way:

- Do some setup, like creating temporary directories and copy files
- Run the `initdb` program
- Start the postgres server
- Wait until postgres has started

The novel idea (as far as I'm aware, although I also only found out about the above python/go libraries after coming up with the initial library idea) in pgtemp is the CLI/daemon which automatically provides connections to new temporary databases upon each connection.

# Examples

## CLI
```
$ cargo install --all-features pgtemp
$ pgtemp postgresql://localhost:6543/mytestdb
starting pgtemp server at postgresql://postgres:password@localhost:6543/mytestdb
$ psql postgresql://postgres:password@localhost:6543/mytestdb
psql (16.1)
Type "help" for help.

postgres=#
# or you could then e.g. run pytest
$ pytest
```

TODO: See examples/ directory for examples:
- python with flask and sqlalchemy

## Library

```rust
use pgtemp::PgTempDB;
use sqlx::postgres::PgConnection;
use sqlx::prelude::*;

#[tokio::test]
fn cool_db_test() {
    let db = PgTempDB::async_new().await;
    let mut conn = sqlx::postgres::PgConnection::connect(&db.connection_uri())
        .await
        .expect("failed to connect to temp db");

    // ... do the rest of your test

    // db is shut down and files cleaned up upon drop at the end of the test
}
```

See the tests/ directory for complete library usage.

TODO: See examples/ directory for examples:
- axum and sqlx with migrations + connection pool
- axum and diesel with migrations + connection pool
