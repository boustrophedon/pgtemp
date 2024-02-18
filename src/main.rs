//! Main binary for pgtemp. It just reads the arguments via clap and passes them to
//! `PgTempDaemon::from_args`
use clap::Parser;

#[tokio::main]
async fn main() {
    let args = pgtemp::PgTempDaemonArgs::parse();
    pgtemp::PgTempDaemon::from_args(args).await.start().await;
}
