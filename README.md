# pgtemp

pgtemp is a Rust library and binary application that allows you to easily create connections to temporary PostgreSQL databases (technically clusters) for testing.

The pgtemp Rust library allows you to spawn a thread which will run a PostgreSQL server in a temporary directory and get back the host, port, username, and password.

The pgtemp cli allows you to even more simply make temporary connections: Run pgtemp as a daemon and then set use its host and port when connecting to the database in your tests. **pgtemp will then spawn a new postgresql process for each connection it receives** and transparently proxy everything over that connection to the temporary database. Note that this means when you make multiple connections in a single test, changes made in one connection will not be visible in the other connections (even if you set the --persist option).

# Design

pgtemp is a fairly simple program and there are other existing libraries like [testing.postgresql for Python](https://github.com/tk0miya/testing.postgresql) and [pgtest for Go](https://github.com/rubenv/pgtest) that all work the same way:

- Do some setup, like creating temporary directories and copy files
- Run the `initdb` program
- Start the postgres server
- Wait until postgres has started

The novel idea (as far as I'm aware, although I also only found out about the above python/go libraries after coming up with the initial library idea) in pgtemp is the CLI/daemon which automatically provides connections to new temporary databases upon each connection.


## MVP features:
### Rust library
- PgTempDB object which when created, creates a temp directory, spawns a thread and execs postgres in it, (does some setup?). When the struct is dropped, it shuts down the server and deletes the files
  - PgTempDBBuilder builder for PgTempDB
    - maybe just allow all of the stuff [here](https://www.postgresql.org/docs/current/libpq-connect.html) but probably just port/user/pass/dbname
  - maybe separate sync and async versions, but for mvp can just make sync version and then `tokio::block_on` async version
  - detect running as root and do something about it (use postgres or nobody user, or require setting in builder)
- Persist option (does not delete directory when object is dropped)
- disable fsync
    - initdb --no-sync
    - postgres -F
- Tests using crates.io/crates/postgres directly?

- Example code
  - Diesel
  - Sqlx

### pgtemp CLI
- --persist option to keep db files on disk

- Example code
  - Python
  - Go?
  technique: set `PGTEMP_URI` which is first read by Pgtemp to set the parameters, then read by test programs. or just use `DATABASE_URL` i guess

## future features / todo
- Provide custom conf for dbs
    - `postgresql.conf`
    - `pg_hba.conf` for auth setup
- Option/argument to provide pre-set up test database to copy each time

- Option to automatically kill postgres instances after some period of time in case drop is not run

- cache initial files from initdb
    - if there exists a `pgtemp_cache` directory in the directory above the data dir (i.e. in a prefix directory) use the files from there
    - this is probably only really usefule for pgtemp cli because it would be hard to coordinate cleanup if you don't know which thread is running last

- extension support
    - `pg_available_extensions` and just automatically enable everything
    - or allow user to specify in PgTempDBBuilder / pgtemp cli arguments

- support unix socket connections?
- set log file (although partially subsumed by above custom conf options because you can set the log file in postgresql.conf or when passing configs when starting the server (or even just directly via ALTER SYSTEM))

Diesel:
- `diesel setup` creates the migrations directory and the `0000..._diesel_initial_setup` migration, and that code appears to be internal to the diesel CLI
    - it does seem like if you have migrations already (e.g. you did `diesel migration generate foo` without running `diesel setup`, running `diesel migration run` will create the `__diesel_schema_migrations` table for you in addition to running the migrations

So I don't think there's anything that we can do in particular since diesel requires a db to be set up before creating the migrations directory - so either the user already has the migrations directory or they don't have any migrations at all. The only thing we could do is make it easier to run the migrations in a test but ideally the user is already doing that in the code path they're testing, rather than having a separate code path that might diverge from production.

### pgtemp CLI
- don't spin up multiple instances, use 1 instance with multiple databases (`CREATE DATABASE pgtemp-<random>`) to speed up time to connection
    - issue: conflicts with --persist because it would make it very hard to know which db you were using at the time. that's kind of true even without this option though. ideally if you're using pgtemp you want to start it with persist, run only the specific failing test, and then inspect that db.
    - also obvious issue: doesn't work if code is actually using multiple databases and not just relying on the connection being set up beforehand, but I don't think most code is actually doing this.

