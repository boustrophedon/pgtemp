use crate::{PgTempDB, PgTempDBBuilder};

use std::net::SocketAddr;

use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::signal::unix::{signal, SignalKind};

#[cfg(feature = "cli")]
/// Contains the clap args struct
pub mod cli {
    use clap::Parser;
    use std::error::Error;
    use std::path::PathBuf;

    #[derive(Parser, Debug)]
    #[command(author, version)]
    /// pgtemp allows you to spawn temporary postgresql databases for testing.
    /// You provide a connection URI and pgtemp will listen on the given port and proxy each
    /// connection to a new temporary database.
    /// When the connection is disconnected, the database is cleaned up.
    pub struct PgTempDaemonArgs {
        #[arg(long)]
        /// Single mode makes every connection go to the same database, rather than starting a new
        /// one per connection.
        pub single: bool,

        #[arg(long, value_name = "DIR")]
        /// The directory in which all temporary postgres data dirs will be stored.
        pub data_dir_prefix: Option<PathBuf>,

        #[arg(long, value_name = "FILE")]
        /// The sql script to be loaded on startup
        pub load_from: Option<PathBuf>,

        #[arg(long, short = 'o', value_name = "KEY=VAL", value_parser = parse_key_val::<String, String>)]
        /// PostgreSQL server configuration parameters in key=value format to pass on startup. May
        /// be passed multiple times.
        pub server_params: Vec<(String, String)>,

        /// The postgres connection uri to be used by pgtemp clients.
        /// E.g. postgresql://localhost:5432/mytestdb
        pub connection_uri: String,
    }

    // from https://github.com/clap-rs/clap/blob/d681a81dd7f4d7ff71f2e65be26d8f90783f7b40/examples/typed-derive.rs#L47C1-L59C2
    /// Parse a single key-value pair
    fn parse_key_val<T, U>(s: &str) -> Result<(T, U), Box<dyn Error + Send + Sync + 'static>>
    where
        T: std::str::FromStr,
        T::Err: Error + Send + Sync + 'static,
        U: std::str::FromStr,
        U::Err: Error + Send + Sync + 'static,
    {
        let pos = s
            .find('=')
            .ok_or_else(|| format!("invalid KEY=value: no `=` found in `{s}`"))?;
        Ok((s[..pos].parse()?, s[pos + 1..].parse()?))
    }
}

#[cfg(feature = "cli")]
pub use cli::PgTempDaemonArgs;

#[derive(Debug)]
/// A daemon that listens on the given port and creates a new [`PgTempDB`] for each connection it
/// receives, proxying all data to the database. If `single_mode` is activated, all connections are
/// proxied to the same database.
pub struct PgTempDaemon {
    port: u16,
    single_mode: bool,
    builder: PgTempDBBuilder,
    /// preallocated databases to speed up connections.
    // TODO: add config to change how many are preallocated etc
    dbs: Vec<PgTempDB>,
}

impl PgTempDaemon {
    #[cfg(feature = "cli")]
    /// Create a [`PgTempDaemon`] from the command line args given.
    pub async fn from_args(args: PgTempDaemonArgs) -> PgTempDaemon {
        let mut builder = PgTempDBBuilder::from_connection_uri(&args.connection_uri);
        if let Some(data_dir_prefix) = args.data_dir_prefix {
            builder = builder.with_data_dir_prefix(data_dir_prefix);
        }
        if let Some(load_from) = args.load_from {
            builder = builder.load_database(&load_from);
        }
        for (key, value) in args.server_params {
            builder = builder.with_config_param(&key, &value);
            eprintln!("{}={}", key, value);
        }

        let port = builder.get_port_or_set_random();
        let single_mode = args.single;
        let dbs = Vec::new();
        let mut daemon = PgTempDaemon {
            port,
            single_mode,
            builder,
            dbs,
        };
        daemon.allocate_db().await; // Pre-allocate a single db. TODO make configurable

        daemon
    }

    /// Quick start a daemon with default parameters.
    pub async fn async_new(port: u16) -> PgTempDaemon {
        let single_mode = false;
        let builder = PgTempDBBuilder::new();
        let dbs = Vec::new();

        let mut daemon = PgTempDaemon {
            port,
            single_mode,
            builder,
            dbs,
        };
        daemon.allocate_db().await;

        daemon
    }

    /// Pre-initialize a [`PgTempDB`]
    pub async fn allocate_db(&mut self) {
        let mut builder = self.builder.clone();
        // Reset the port so that a port is allocated randomly when we make
        // new dbs
        builder.port = None;

        self.dbs.push(builder.start_async().await);
    }

    fn conn_uri(&self) -> String {
        format!(
            "postgresql://{}:{}@localhost:{}/{}",
            self.builder.get_user(),
            self.builder.get_password(),
            self.port,
            self.builder.get_dbname()
        )
    }

    /// Start the daemon, listening for either TCP connections on the configured port. The server
    /// shuts down when sent a SIGINT (e.g. via ctrl-C).
    pub async fn start(mut self) {
        let uri = self.conn_uri();
        if self.single_mode {
            println!("starting pgtemp server in single mode at {}", uri);
        } else {
            println!("starting pgtemp server at {}", uri);
        }

        let listener = TcpListener::bind(("127.0.0.1", self.port))
            .await
            .expect("failed to bind to daemon port");
        let mut sig = signal(SignalKind::interrupt()).expect("failed to hook to interrupt signal");
        loop {
            tokio::select! {
                res = listener.accept() => {
                    if let Ok((client_conn, client_addr)) = res {
                        client_conn.set_nodelay(true).expect("failed to set nodelay on client connection");
                        let db: Option<PgTempDB>;
                        let db_port: u16;
                        if self.single_mode {
                            db = None;
                            db_port = self.dbs[0].db_port();
                        }
                        else {
                            let take_db = self.dbs.pop().unwrap();
                            db_port = take_db.db_port();
                            db = Some(take_db);
                        }
                        let db_conn = TcpStream::connect(("127.0.0.1", db_port))
                            .await
                            .expect("failed to connect to postgres server");
                        db_conn
                            .set_nodelay(true)
                            .expect("failed to set nodelay on db connection");
                        tokio::spawn(async move { proxy_connection(db, db_conn, client_conn, client_addr).await });
                        // preallocate a new db after one is used
                        if self.dbs.is_empty() && !self.single_mode {
                            self.allocate_db().await;
                        }
                    }
                    else {
                        println!("idk when this errs");
                    }
                }
                _sig_event = sig.recv() => {
                    println!("got interrupt, exiting");
                    break;
                }
            }
        }
    }
}

/// When we're in single mode, we pass None to the db here so it doesn't get deallocated when the
/// connection is closed, and when we're not in single mode we pass the PgTempDB inside the option
/// so that it gets dropped when the connection is dropped.
async fn proxy_connection(
    _db: Option<PgTempDB>,
    mut db_conn: TcpStream,
    mut client_conn: TcpStream,
    _client_addr: SocketAddr,
) {
    loop {
        tokio::select! {
            _ = db_conn.readable() => {
                let mut buf = [0; 4096];
                match db_conn.try_read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        client_conn.write_all(&buf[0..n]).await
                            .expect("failed to write to client connection");
                    }
                    Err(ref e) if e.kind() == tokio::io::ErrorKind::WouldBlock => {
                        continue;
                    }
                    Err(e) => {
                        panic!("error reading from client socket: {:?}", e);
                    }
                }
            },
            _ = client_conn.readable() => {
                let mut buf = [0; 4096];
                match client_conn.try_read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        db_conn.write_all(&buf[0..n]).await
                            .expect("failed to write to db connection");
                    }
                    Err(ref e) if e.kind() == tokio::io::ErrorKind::WouldBlock => {
                        continue;
                    }
                    Err(e) => {
                        panic!("error reading from db socket: {:?}", e);
                    }
                }
            },
        }
    }
}
