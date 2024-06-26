# pgtemp

[![Coverage Status](https://coveralls.io/repos/github/boustrophedon/pgtemp/badge.svg?branch=master)](https://coveralls.io/github/boustrophedon/pgtemp?branch=master) [![CI Status](https://github.com/boustrophedon/pgtemp/actions/workflows/build-test.yaml/badge.svg)](https://github.com/boustrophedon/pgtemp/actions/workflows/build-test.yaml) [![crates.io](https://img.shields.io/crates/v/pgtemp)](https://crates.io/crates/pgtemp) [![docs.rs](https://img.shields.io/docsrs/pgtemp)](https://docs.rs/pgtemp/latest/pgtemp/)

pgtemp is a Rust library and cli tool that allows you to easily create temporary PostgreSQL servers for testing without using Docker.

The pgtemp Rust library allows you to spawn a PostgreSQL server in a temporary directory and get back a full connection URI with the host, port, username, and password.

The pgtemp cli tool allows you to even more simply make temporary connections, and works with any language: Run pgtemp and then use its connection URI when connecting to the database in your tests. **pgtemp will then spawn a new postgresql process for each connection it receives** and transparently proxy everything over that connection to the temporary database. Note that this means when you make multiple connections in a single test, changes made in one connection will not be visible in the other connections, unless you are using pgtemp's `--single` mode.

pgtemp supports loading (and dumping, in the library) the database to/from [dumpfiles via `pg_dump`](https://www.postgresql.org/docs/current/backup-dump.html).

Note that the default postgres authentication configuration (`pg_hba.conf`) in most cases allows all local connections. Since pgtemp only allows you to make servers that listen on localhost, this means in most cases you do not need to provide a password to connect. You may set the server's `hba_file` parameter in `PgTempDBBuilder::with_config_param` or use the pgtemp daemon's `-o` flag to pass `hba_file` there.

# Requirements
You must install both the postgresql client and server packages. On Debian/Ubuntu, they are `postgresql postgresql-client`, on Fedora they are `postgresql postgresql-server`, and on Arch Linux they are `postgresql postgresql-libs`. Note also that Debian/Ubuntu install the standard postgres binaries into their own directory, so you must add them to your path. For an Ubuntu GitHub Actions runner, it looks like:

```
steps:
  - name: Install postgres
    run: sudo apt-get install postgresql postgresql-client
  - name: Update path
    run: find /usr/lib/postgresql/ -type d -name "bin" >> $GITHUB_PATH
```

To install the CLI tool, you must install it with the --features cli or --all-features options
```
cargo install pgtemp --features cli
```

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
# username, password, port, and database name are all configurable based on the provided connection URI
$ pgtemp postgresql://localhost:6543/mytestdb 
starting pgtemp server at postgresql://postgres:password@localhost:6543/mytestdb
$ psql postgresql://postgres:password@localhost:6543/mytestdb
psql (16.1)
Type "help" for help.

postgres=#
```

See examples/ directory for examples:
- A python example with sqlalchemy and alembic, demonstrating usage with the pgtemp cli's normal and single modes

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

Examples:
- A simple diesel example with axum
- A more complicated "task queue" example using triggers and LISTEN/NOTIFY with sqlx and axum

See the tests/ directory for complete library usage.
