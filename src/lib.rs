#![warn(missing_docs)] // denied in CI

//! pgtemp is a Rust library and cli tool that allows you to easily create temporary PostgreSQL servers for testing without using Docker.
//!
//! The pgtemp Rust library allows you to spawn a PostgreSQL server in a temporary directory and get back a full connection URI with the host, port, username, and password.
//!
//! The pgtemp cli tool allows you to even more simply make temporary connections, and works with any language: Run pgtemp and then use its connection URI when connecting to the database in your tests. **pgtemp will then spawn a new postgresql process for each connection it receives** and transparently proxy everything over that connection to the temporary database. Note that this means when you make multiple connections in a single test, changes made in one connection will not be visible in the other connections, unless you are using pgtemp's `--single` mode.

use std::collections::HashMap;
use std::fmt;
use std::fmt::Debug;
use std::path::{Path, PathBuf};
use std::process::Child;

use tempfile::TempDir;
use tokio::task::spawn_blocking;

mod daemon;
mod run_db;

pub use daemon::*;

// temp db handle - actual db spawning code is in run_db mod

/// A struct representing a handle to a local PostgreSQL server that is currently running. Upon
/// drop or calling `shutdown`, the server is shut down and the directory its data is stored in
/// is deleted. See builder struct [`PgTempDBBuilder`] for options and settings.
pub struct PgTempDB {
    dbuser: String,
    dbpass: String,
    dbport: u16,
    dbname: String,
    /// persist the db data directory after shutdown
    persist: bool,
    /// dump the databaset to a script file after shutdown
    dump_path: Option<PathBuf>,
    // See shutdown implementation for why these are options
    temp_dir: Option<TempDir>,
    postgres_process: Option<Child>,
}

impl PgTempDB {
    /// Start a PgTempDB with the parameters configured from a PgTempDBBuilder
    pub fn from_builder(mut builder: PgTempDBBuilder) -> PgTempDB {
        let dbuser = builder.get_user();
        let dbpass = builder.get_password();
        let dbport = builder.get_port_or_set_random();
        let dbname = builder.get_dbname();
        let persist = builder.persist_data_dir;
        let dump_path = builder.dump_path.clone();
        let load_path = builder.load_path.clone();

        let temp_dir = run_db::init_db(&mut builder);
        let postgres_process = Some(run_db::run_db(&temp_dir, builder));
        let temp_dir = Some(temp_dir);

        let db = PgTempDB {
            dbuser,
            dbpass,
            dbport,
            dbname,
            persist,
            dump_path,
            temp_dir,
            postgres_process,
        };

        if let Some(path) = load_path {
            db.load_database(path);
        }
        db
    }

    /// Creates a builder that can be used to configure the details of the temporary PostgreSQL
    /// server
    pub fn builder() -> PgTempDBBuilder {
        PgTempDBBuilder::new()
    }

    /// Creates a new PgTempDB with default configuration and starts a PostgreSQL server.
    pub fn new() -> PgTempDB {
        PgTempDBBuilder::new().start()
    }

    /// Creates a new PgTempDB with default configuration and starts a PostgreSQL server in an
    /// async context.
    pub async fn async_new() -> PgTempDB {
        PgTempDBBuilder::new().start_async().await
    }

    /// Use [pg_dump](https://www.postgresql.org/docs/current/backup-dump.html) to dump the
    /// database to the provided path upon drop or [`Self::shutdown`].
    pub fn dump_database(&self, path: impl AsRef<Path>) {
        let path_str = path.as_ref().to_str().unwrap();

        let dump_output = std::process::Command::new("pg_dump")
            .arg(self.connection_uri())
            .args(["--file", path_str])
            .output()
            .expect("failed to start pg_dump. Is it installed and on your path?");

        if !dump_output.status.success() {
            let stdout = dump_output.stdout;
            let stderr = dump_output.stderr;
            panic!(
                "pg_dump failed! stdout: {}\n\nstderr: {}",
                String::from_utf8_lossy(&stdout),
                String::from_utf8_lossy(&stderr)
            );
        }
    }

    /// Use `psql` to load the database from the provided dump file. See [`Self::dump_database`].
    pub fn load_database(&self, path: impl AsRef<Path>) {
        let path_str = path.as_ref().to_str().unwrap();

        let load_output = std::process::Command::new("psql")
            .arg(self.connection_uri())
            .args(["--file", path_str])
            .output()
            .expect("failed to start psql. Is it installed and on your path?");

        if !load_output.status.success() {
            let stdout = load_output.stdout;
            let stderr = load_output.stderr;
            panic!(
                "psql failed! stdout: {}\n\nstderr: {}",
                String::from_utf8_lossy(&stdout),
                String::from_utf8_lossy(&stderr)
            );
        }
    }

    /// Send a signal to the database to shutdown the server, then wait for the process to exit.
    /// Equivalent to calling drop on this struct.
    ///
    /// NOTE: This is currently a blocking function. It sends SIGKILL and waits for the process to
    /// exit, and also does IO to remove the temp directory. This shouldn't matter since even in
    /// async test functions it's only going to run at the end of the test, and it shouldn't
    /// block indefinitely.
    ///
    /// However, if the persist option is set, this function will attempt to shut down postgres
    /// gracefully by sending it a SIGTERM. Postgres will not shut down until all clients have
    /// disconnected, so if you call shutdown while a connection is still active (e.g. by dropping
    /// the database manually) this function will hang indefinitely. In most cases this shouldn't
    /// be an issue because objects are dropped in reverse order of creation, so a connection will
    /// be dropped before the database.
    pub fn shutdown(&mut self) {
        // TODO: I believe that the spawned thread below isn't getting time to run for the last
        // test that executes in a given process, so we leak tempdbs in the filesystem. I don't
        // know a good way to prevent this besides sleeping so I think the next best thing is to
        // just block (the postgress_process.wait and all the fs operations TempDir::close does)
        // even if we're in an async context.
        //
        // // Since the Drop trait is not async but we have to wait for the postgres process to exit
        // // before deleting the directory, move the postgres server process and temp dir structs
        // // into a "cleanup" thread

        // let mut postgres_process = self.postgres_process.take().unwrap();
        // let temp_dir = self.temp_dir.take().unwrap();
        // std::thread::spawn(move || {
        //     // graceful shutdown
        //     // let _ret = unsafe { libc::kill(postgres_process.id() as i32, libc::SIGTERM) };
        //     postgres_process.wait().expect("postgres server failed to exit cleanly");
        //     if persist {
        //         // this prevents it from being auto deleted
        //         let _path = temp_dir.into_path();
        //     }
        //     else {
        //         drop(temp_dir);
        //     }
        // });

        // do the dump while the postgres process is still running
        if let Some(path) = &self.dump_path {
            self.dump_database(path);
        }

        let mut postgres_process = self.postgres_process.take().unwrap();
        let temp_dir = self.temp_dir.take().unwrap();

        if self.persist {
            // graceful shutdown if we're trying to persist.
            #[allow(clippy::cast_possible_wrap)]
            let _ret = unsafe { libc::kill(postgres_process.id() as i32, libc::SIGTERM) };
            let _output = postgres_process
                .wait_with_output()
                .expect("postgres server failed to exit cleanly");
            // this prevents it from being deleted on drop
            let _path = temp_dir.into_path();
        } else {
            // If there are clients connected the server will not shut down until they are
            // disconnected, so we should just send sigkill and end it immediately.
            postgres_process
                .kill()
                .expect("postgres server could not be killed");
            let _output = postgres_process
                .wait_with_output()
                .expect("postgres server failed to exit cleanly");
            temp_dir.close().expect("failed to clean up temp directory");
        }
    }

    /// Returns the path to the data directory being used by this databaset.
    pub fn data_dir(&self) -> PathBuf {
        self.temp_dir.as_ref().unwrap().path().join("pg_data_dir")
    }

    /// Returns the database username used when connecting to the postgres server.
    pub fn db_user(&self) -> &str {
        &self.dbuser
    }

    /// Returns the database password used when connecting to the postgres server.
    pub fn db_pass(&self) -> &str {
        &self.dbpass
    }

    /// Returns the port the postgres server is running on.
    pub fn db_port(&self) -> u16 {
        self.dbport
    }

    /// Returns the the name of the database created.
    pub fn db_name(&self) -> &str {
        &self.dbname
    }

    /// Returns a connection string that can be passed to a libpq connection function.
    ///
    /// Example output:
    /// `host=localhost port=15432 user=pgtemp password=pgtemppw-9485 dbname=pgtempdb-324`
    pub fn connection_string(&self) -> String {
        format!(
            "host=localhost port={} user={} password={} dbname={}",
            self.db_port(),
            self.db_user(),
            self.db_pass(),
            self.db_name()
        )
    }

    /// Returns a generic connection URI that can be passed to most SQL libraries' connect
    /// methods.
    ///
    /// Example output:
    /// `postgresql://pgtemp:pgtemppw-9485@localhost:15432/pgtempdb-324`
    pub fn connection_uri(&self) -> String {
        format!(
            "postgresql://{}:{}@localhost:{}/{}",
            self.db_user(),
            self.db_pass(),
            self.db_port(),
            self.db_name()
        )
    }
}

impl Debug for PgTempDB {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PgTempDB")
            .field("base directory", self.temp_dir.as_ref().unwrap())
            .field("connection string", &self.connection_string())
            .field("persist data dir", &self.persist)
            .field("dump path", &self.dump_path)
            .field(
                "db process",
                &self.postgres_process.as_ref().map(Child::id).unwrap(),
            )
            .finish_non_exhaustive()
    }
}

impl Drop for PgTempDB {
    fn drop(&mut self) {
        self.shutdown();
    }
}

// db config builder functions

/// Builder struct for PgTempDB.
#[derive(Debug, Clone)]
pub struct PgTempDBBuilder {
    /// The directory in which to store the temporary PostgreSQL data directory.
    pub temp_dir_prefix: Option<PathBuf>,
    /// The cluster superuser created with `initdb`. Default: `postgres`
    pub db_user: Option<String>,
    /// The password for the cluster superuser. Default: `password`
    pub password: Option<String>,
    /// The port the server should run on. Default: random unused port.
    pub port: Option<u16>,
    /// The name of the database to create on startup. Default: `postgres`.
    pub dbname: Option<String>,
    /// Do not delete the data dir when the `PgTempDB` is dropped.
    pub persist_data_dir: bool,
    /// The path to dump the database to (via `pg_dump`) when the `PgTempDB` is dropped.
    pub dump_path: Option<PathBuf>,
    /// The path to load the database from (via `psql`) when the `PgTempDB` is started.
    pub load_path: Option<PathBuf>,
    /// Other server configuration data to be set in `postgresql.conf` via `initdb -c`
    pub server_configs: HashMap<String, String>,
}

impl PgTempDBBuilder {
    /// Create a new [`PgTempDBBuilder`]
    pub fn new() -> PgTempDBBuilder {
        PgTempDBBuilder {
            temp_dir_prefix: None,
            db_user: None,
            password: None,
            port: None,
            dbname: None,
            persist_data_dir: false,
            dump_path: None,
            load_path: None,
            server_configs: HashMap::new(),
        }
    }

    /// Parses the parameters out of a PostgreSQL connection URI and inserts them into the builder.
    #[must_use]
    pub fn from_connection_uri(conn_uri: &str) -> Self {
        let mut builder = PgTempDBBuilder::new();

        let url = url::Url::parse(conn_uri)
            .expect(&format!("Could not parse connection URI `{}`", conn_uri));

        // TODO: error types
        assert!(
            url.scheme() == "postgresql",
            "connection URI must start with `postgresql://` scheme: `{}`",
            conn_uri
        );
        assert!(
            url.host_str() == Some("localhost"),
            "connection URI's host is not localhost: `{}`",
            conn_uri,
        );

        let username = url.username();
        let password = url.password();
        let port = url.port();
        let dbname = url.path().strip_prefix('/').unwrap_or("");

        if !username.is_empty() {
            builder = builder.with_username(username);
        }
        if let Some(password) = password {
            builder = builder.with_password(password);
        }
        if let Some(port) = port {
            builder = builder.with_port(port);
        }
        if !dbname.is_empty() {
            builder = builder.with_dbname(dbname);
        }

        builder
    }

    // TODO: make an error type and `try_start` methods (and maybe similar for above shutdown etc
    // functions)

    /// Creates the temporary data directory and starts the PostgreSQL server with the configured
    /// parameters.
    ///
    /// If the current user is root, will attempt to run the `initdb` and `postgres` commands as
    /// the `postgres` user.
    pub fn start(self) -> PgTempDB {
        PgTempDB::from_builder(self)
    }

    /// Convenience function for calling `spawn_blocking(self.start())`
    pub async fn start_async(self) -> PgTempDB {
        spawn_blocking(move || self.start())
            .await
            .expect("failed to start pgtemp server")
    }

    /// Set the directory in which to put the (temporary) PostgreSQL data directory. This is not
    /// the data directory itself: a new temporary directory is created inside this one.
    #[must_use]
    pub fn with_data_dir_prefix(mut self, prefix: impl AsRef<Path>) -> Self {
        self.temp_dir_prefix = Some(PathBuf::from(prefix.as_ref()));
        self
    }

    /// Set an arbitrary PostgreSQL server configuration parameter that will passed to the
    /// postgresql process at runtime.
    #[must_use]
    pub fn with_config_param(mut self, key: &str, value: &str) -> Self {
        let _old = self.server_configs.insert(key.into(), value.into());
        self
    }

    #[must_use]
    /// Set the user name
    pub fn with_username(mut self, username: &str) -> Self {
        self.db_user = Some(username.to_string());
        self
    }

    #[must_use]
    /// Set the user password
    pub fn with_password(mut self, password: &str) -> Self {
        self.password = Some(password.to_string());
        self
    }

    #[must_use]
    /// Set the port
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    #[must_use]
    /// Set the database name
    pub fn with_dbname(mut self, dbname: &str) -> Self {
        self.dbname = Some(dbname.to_string());
        self
    }

    /// If set, the postgres data directory will not be deleted when the `PgTempDB` is dropped.
    #[must_use]
    pub fn persist_data(mut self, persist: bool) -> Self {
        self.persist_data_dir = persist;
        self
    }

    /// If set, the database will be dumped via the `pg_dump` utility to the given location on drop
    /// or upon calling [`PgTempDB::shutdown`].
    #[must_use]
    pub fn dump_database(mut self, path: &Path) -> Self {
        self.dump_path = Some(path.into());
        self
    }

    /// If set, the database will be loaded via `psql` from the given script on startup.
    #[must_use]
    pub fn load_database(mut self, path: &Path) -> Self {
        self.load_path = Some(path.into());
        self
    }

    /// Get user if set or return default
    pub fn get_user(&self) -> String {
        self.db_user.clone().unwrap_or(String::from("postgres"))
    }

    /// Get password if set or return default
    pub fn get_password(&self) -> String {
        self.password.clone().unwrap_or(String::from("password"))
    }

    /// Unlike the other getters, this getter will try to open a new socket to find an unused port,
    /// and then set it as the current port.
    pub fn get_port_or_set_random(&mut self) -> u16 {
        let port = self.port.as_ref().copied().unwrap_or_else(get_unused_port);

        self.port = Some(port);
        port
    }

    /// Get dbname if set or return default
    pub fn get_dbname(&self) -> String {
        self.dbname.clone().unwrap_or(String::from("postgres"))
    }
}

fn get_unused_port() -> u16 {
    // TODO: relies on Rust's stdlib setting SO_REUSEPORT by default so that postgres can still
    // bind to the port afterwards. Also there's a race condition/TOCTOU because there's lag
    // between when the port is checked here and when postgres actually tries to bind to it.
    let sock = std::net::TcpListener::bind("localhost:0")
        .expect("failed to bind to local port when getting unused port");
    sock.local_addr()
        .expect("failed to get local addr from socket")
        .port()
}
