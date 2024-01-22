use std::collections::HashMap;
use std::fmt;
use std::fmt::Debug;
use std::path::PathBuf;
use std::process::Child;

use tokio::task::spawn_blocking;
use tempfile::TempDir;

mod run_db;

// temp db handle - actual db spawning code is in run_db mod

/// A struct representing a handle to a local PostgreSQL database that is currently running. Upon
/// drop or calling `shutdown`, the database is shut down and the directory its data is stored in
/// is deleted.
pub struct PgTempDB {
    dbuser: String,
    dbpass: String,
    dbport: u16,
    dbname: String,
    // temp_dir and postgres_process are Options so that we can `take()` them when calling
    // `shutdown` and move them into a cleanup thread
    temp_dir: Option<TempDir>,
    postgres_process: Option<Child>,
}

impl PgTempDB {
    /// Constructor
    pub(crate) fn new_with(dbuser: String, dbpass: String, dbport: u16, dbname: String, temp_dir: TempDir, postgres_process: Child) -> PgTempDB {
        let temp_dir = Some(temp_dir);
        let postgres_process = Some(postgres_process);
        PgTempDB {
            dbuser,
            dbpass,
            dbport,
            dbname,
            temp_dir,
            postgres_process
        }
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

    /// Send a signal to the database to shutdown the server. Does not delete the database
    /// data directory.
    fn shutdown(&mut self, persist: bool) {
        // Since the Drop trait is not async but we have to wait for the postgres process to exit
        // before deleting the directory, move the postgres server process and temp dir structs
        // into a "cleanup" thread

        let mut postgres_process = self.postgres_process.take().unwrap();
        let temp_dir = self.temp_dir.take().unwrap();
        std::thread::spawn(move || {
            let _ret = unsafe { libc::kill(postgres_process.id() as i32, libc::SIGTERM) };
            postgres_process.wait().expect("postgres server failed to exit cleanly");
            if persist {
                // this prevents it from being auto deleted
                let _path = temp_dir.into_path();
            }
            else {
                drop(temp_dir);
            }
        });
    }

    /// Shuts down the database but does not delete the data directory. This may be useful for
    /// creating an initial test data set or for debugging purposes.
    pub fn shutdown_and_persist(mut self) {
        self.shutdown(true);
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
        format!("host=localhost port={} user={} password={} dbname={}", self.db_port(), self.db_user(), self.db_pass(), self.db_name())
    }

    /// Returns a generic connection URI that can be passed to most SQL libraries' connect
    /// methods.
    ///
    /// Example output:
    /// `postgresql://pgtemp:pgtemppw-9485@localhost:15432/pgtempdb-324`
    pub fn connection_uri(&self) -> String {
        format!("postgresql://{}:{}@localhost:{}/{}", self.db_user(), self.db_pass(), self.db_port(), self.db_name())
    }
}

impl Debug for PgTempDB {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PgTempDB")
         .field("directory", self.temp_dir.as_ref().unwrap())
         .field("connection string", &self.connection_string())
         .finish()
    }
}

impl Drop for PgTempDB {
    fn drop(&mut self) {
        self.shutdown(false);
    }
}


// db config builder functions

/// Builder struct for PgTempDB.
///
/// Defaults:
/// temp dir prefix: `std::env::temp_dir()`, inside which `pgtemp-<random>/pg_data_dir` is created.
/// db_user: `postgres`
/// password: `password`
/// port: `<random unused port>` set when `PgTempDBBuilder::start` is called.
/// dbname: `postgres` - created by initdb default
pub struct PgTempDBBuilder {
    /// The directory in which to store the temporary PostgreSQL data directory
    pub temp_dir_prefix: Option<PathBuf>,
    /// The cluster superuser created with `initdb`
    pub db_user: Option<String>,
    /// The password for the cluster superuser
    pub password: Option<String>,
    /// The port the server should run on.
    pub port: Option<u16>,
    /// The name of the database to create on startup
    pub dbname: Option<String>,
    /// Other server configuration data to be set in `postgresql.conf` via `initdb -c`
    pub server_configs: HashMap<String, String>,
}

impl PgTempDBBuilder {
    pub fn new() -> PgTempDBBuilder {
        PgTempDBBuilder {
            temp_dir_prefix: None,
            db_user: None,
            password: None,
            port: None,
            dbname: None,
            server_configs: HashMap::new(),
        }
    }

    /// Set the directory in which to put the (temporary) PostgreSQL data directory. This is not
    /// the data directory itself: a new temporary directory is created inside this one.
    pub fn with_data_dir_prefix(mut self, prefix: &str) -> Self {
        self.temp_dir_prefix = Some(PathBuf::from(prefix));
        self
    }

    /// Set an arbitrary PostgreSQL server configuration parameter that will be inserted into
    /// `postgresql.conf` by initdb.
    pub fn with_config_param(mut self, key: &str, value: &str) -> Self {
        let _old = self.server_configs.insert(key.into(), value.into());
        self
    }

    /// Parses the parameters out of a PostgreSQL connection URI and inserts them into the builder.
    pub fn from_connection_uri(mut self, conn_uri: &str) -> Self {
        let url = url::Url::parse(conn_uri).expect(&format!("Could not parse connection URI `{}`", conn_uri));
        if url.scheme() != "postgres" {
            panic!("connection URI is not a postgres connection URI: `{}`", conn_uri);
        }
        if url.host_str() != Some("localhost") {
            panic!("connection URI's host is not localhost: `{}`", conn_uri);
        }

        let username = url.username();
        let password = url.password();
        let port = url.port();
        let dbname = url.path();

        if username != "" {
            self = self.with_username(username.into());
        }
        if let Some(password) = password {
            self = self.with_password(password.into());
        }
        if let Some(port) = port {
            self = self.with_port(port.into());
        }
        if dbname != "" {
            self = self.with_dbname(dbname.into())
        }

        self
    }

    // TODO: make an error type and `try_start` methods (and maybe similar for above shutdown etc
    // functions)

    /// Creates the temporary data directory and starts the PostgreSQL server with the configured
    /// parameters.
    ///
    /// If the current user is root, will attempt to run the `initdb` and `postgres` commands as
    /// the `postgres` user.
    pub fn start(mut self) -> PgTempDB {
        let temp_dir = run_db::init_db(&mut self);
        run_db::run_db(temp_dir, self)
    }

    /// Convenience function for calling `spawn_blocking(self.start())`
    pub async fn start_async(self) -> PgTempDB {
        spawn_blocking(move || self.start()).await
            .expect("failed to start pgtemp server")
    }

    pub fn with_username(mut self, username: String) -> Self {
        self.db_user = Some(username);
        self
    }

    pub fn with_password(mut self, password: String) -> Self {
        self.password = Some(password);
        self
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    pub fn with_dbname(mut self, dbname: String) -> Self {
        self.dbname = Some(dbname);
        self
    }

    /// Get user if set or return default
    pub fn get_user(&self) -> String {
        self.db_user.as_ref()
            .cloned()
            .unwrap_or(String::from("postgres"))
    }

    /// Get password if set or return default
    pub fn get_password(&self) -> String {
        self.password.as_ref()
            .cloned()
            .unwrap_or(String::from("password"))
    }

    /// Unlike the other getters, this getter will try to open a new socket to find an unused port,
    /// and then set it as the current port.
    pub fn get_port_or_set_random(&mut self) -> u16 {
        let port = self.port.as_ref()
            .copied()
            .unwrap_or_else(get_unused_port);

        self.port = Some(port);
        port
    }

    /// Get dbname if set or return default
    pub fn get_dbname(&self) -> String {
        self.dbname.as_ref()
            .cloned()
            .unwrap_or(String::from("postgres"))
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

